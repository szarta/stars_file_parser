// hst_to_json — read a Stars! .hst host file and emit the full universe state.
//
// Usage:  hst_to_json <file.hst>
//
// The host file is the omniscient, authoritative record of a game: unlike a
// player turn file (.mN), it holds a type-6 PlayerBlock for EVERY player and a
// type-13 PlanetBlock for EVERY planet (no scanner fog).  This tool emits a
// universe-centric schema — collections of players, planets, fleets, designs,
// battle plans, and raw host-only objects — rather than the player-centric
// shape of m1_to_json.
//
// Output JSON (top-level):
//   "year"          — game year (type-8 header: turn + 2400)
//   "player_count"  — number of full PlayerBlocks decoded
//   "players"       — one entry per type-6 PlayerBlock (id, homeworld, tech, race)
//   "planet_count"  — number of type-13 PlanetBlocks
//   "planets"       — every planet in the universe (owner_id 31 = uninhabited;
//                     a planet is a homeworld iff its index appears as some
//                     player's homeworld_planet_idx — authoritative here because
//                     the host file holds every player's block)
//   "fleets"        — every fleet (type-16 FleetBlock), owner-tagged
//   "designs"       — every ship/starbase design (type-26 DesignBlock)
//   "battle_plans"  — every battle plan (type-30 BattlePlanBlock)
//   "objects"       — raw type-43 ObjectBlock records (Mystery Trader / space
//                     objects); layout not yet decoded, so bytes are surfaced
//                     as hex per ADR-0001 rather than guessed.
//   "record_summary"— count per record type seen, so host-only record types
//                     beyond the decoded set are visible at a glance.
//
// The block decoders are shared with m1_to_json via `stars_file_parser::blocks`.

use std::collections::BTreeMap;
use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::{
    blocks::{
        decode_type13, decode_type16, decode_type26, decode_type30, decode_type6, BattlePlanRecord,
        DesignRecord, FleetRecord, PlanetRecord,
    },
    race::Race,
    records::{game_year, parse_file},
};

// ── Output types (universe-centric host view) ──────────────────────────────────

/// One player's state from a type-6 PlayerBlock.
#[derive(Debug, Serialize)]
struct PlayerEntry {
    /// 0-based player ID (type-6 byte 0).
    player_id: u8,
    /// Homeworld planet index (type-6 bytes 8-9 LE uint16).  Confirmed to match
    /// each player's owned planet across all PlayerBlocks in a .hst.
    homeworld_planet_idx: Option<u16>,
    // NOTE: m1_to_json exposes a `planet_count` from type-6 bytes 2-3, confirmed
    // for the active player's own block.  It is deliberately omitted here: in a
    // .hst it reads the universe planet total for the host's own block and 0 for
    // other players' blocks, so it is not a per-player owned count.  Emitting it
    // would be a confidently-wrong decode (ADR-0001).  Owned counts can be derived
    // from `planets` by `owner_id` instead.
    tech_energy: Option<u8>,
    tech_weapons: Option<u8>,
    tech_propulsion: Option<u8>,
    tech_construction: Option<u8>,
    tech_electronics: Option<u8>,
    tech_biology: Option<u8>,
    /// Full race spec (bytes 16-81 + name section at 112+).
    #[serde(skip_serializing_if = "Option::is_none")]
    race: Option<Race>,
}

/// A host-only record whose layout is not yet decoded; bytes surfaced as hex.
#[derive(Debug, Serialize)]
struct RawRecord {
    /// Record type (e.g. 43 = ObjectBlock / Mystery Trader).
    rtype: u16,
    /// Payload length in bytes.
    len: usize,
    /// Payload bytes as a lowercase hex string (ADR-0001: surface, don't guess).
    hex: String,
}

/// Full omniscient host-file state.
#[derive(Debug, Serialize)]
struct HostState {
    /// Game year (type-8 header: turn + 2400).
    year: u32,
    player_count: usize,
    players: Vec<PlayerEntry>,
    planet_count: usize,
    planets: Vec<PlanetRecord>,
    /// Every fleet in the universe (type-16 FleetBlock), owner-tagged by `owner_id`.
    fleets: Vec<FleetRecord>,
    /// Every ship/starbase design (type-26 DesignBlock).
    designs: Vec<DesignRecord>,
    /// Every battle plan (type-30 BattlePlanBlock), owner-tagged via `plan_id`'s player bits.
    battle_plans: Vec<BattlePlanRecord>,
    /// Raw type-43 ObjectBlock records (Mystery Trader / space objects), undecoded.
    objects: Vec<RawRecord>,
    /// Count of records seen per record type (diagnostic; includes types not decoded above).
    record_summary: BTreeMap<u16, usize>,
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: hst_to_json <file.hst>");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    });

    let records = parse_file(&bytes).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    let year = game_year(&records).unwrap_or(2400);

    let mut players: Vec<PlayerEntry> = Vec::new();
    let mut planets: Vec<PlanetRecord> = Vec::new();
    let mut fleets: Vec<FleetRecord> = Vec::new();
    let mut designs: Vec<DesignRecord> = Vec::new();
    let mut battle_plans: Vec<BattlePlanRecord> = Vec::new();
    let mut objects: Vec<RawRecord> = Vec::new();
    let mut record_summary: BTreeMap<u16, usize> = BTreeMap::new();

    for rec in &records {
        *record_summary.entry(rec.rtype).or_insert(0) += 1;
        match rec.rtype {
            6 => {
                // The host file carries a full PlayerBlock per player.  Skip
                // partial/stub blocks (< 32 bytes) that lack the tech/race spec.
                if rec.payload.len() < 32 {
                    continue;
                }
                let pb = decode_type6(&rec.payload);
                players.push(PlayerEntry {
                    player_id: pb.player_id,
                    homeworld_planet_idx: pb.homeworld_planet_idx,
                    tech_energy: pb.techs[0],
                    tech_weapons: pb.techs[1],
                    tech_propulsion: pb.techs[2],
                    tech_construction: pb.techs[3],
                    tech_electronics: pb.techs[4],
                    tech_biology: pb.techs[5],
                    race: pb.race,
                });
            }
            13 => {
                if let Some(planet) = decode_type13(&rec.payload) {
                    planets.push(planet);
                }
            }
            16 => {
                if let Some(fleet) = decode_type16(&rec.payload) {
                    fleets.push(fleet);
                }
            }
            26 => {
                if let Some(d) = decode_type26(&rec.payload) {
                    designs.push(d);
                }
            }
            30 => {
                if let Some(bp) = decode_type30(&rec.payload) {
                    battle_plans.push(bp);
                }
            }
            43 => {
                objects.push(RawRecord {
                    rtype: rec.rtype,
                    len: rec.payload.len(),
                    hex: to_hex(&rec.payload),
                });
            }
            _ => {}
        }
    }

    let state = HostState {
        year,
        player_count: players.len(),
        players,
        planet_count: planets.len(),
        planets,
        fleets,
        designs,
        battle_plans,
        objects,
        record_summary,
    };

    let json = serde_json::to_string_pretty(&state).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_is_lowercase_padded() {
        assert_eq!(to_hex(&[0x00, 0x2b, 0xff, 0x07]), "002bff07");
        assert_eq!(to_hex(&[]), "");
    }
}
