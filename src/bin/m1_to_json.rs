// m1_to_json — read a Stars! .m1 player turn file and emit turn JSON on stdout.
//
// Usage:  m1_to_json <file.m1>
//
// Output is a JSON object with the following top-level keys:
//   "year"      — game year decoded from type-8 file header (turn + 2400)
//   "player"    — player state from type-6 PlayerBlock (tech levels, homeworld idx)
//   "planets"   — list of planets visible to the player (from type-13 PlanetBlock)
//   "fleets"    — fleet records (from type-16 FleetBlock)
//   "waypoints" — waypoint records (from type-19 TaskWaypointBlock and type-20 WaypointBlock)
//   "designs"   — ship/starbase designs (type-26 DesignBlock)
//   "events"    — per-turn event log (type-12 EventsBlock, partial decode)
//
// This is the primary oracle comparison tool for Phase R2–R5 research.
// The block decoders live in `stars_file_parser::blocks`; this tool wraps them
// in the player-centric turn-file view (one owning player, selected from the
// filename).  The omniscient multi-player host view is `hst_to_json`.
//
// Record type name corrections (from XyliGUN's block type registry, 2011):
//   type-16 = FleetBlock, type-19 = TaskWaypointBlock, type-20 = WaypointBlock,
//   type-26 = DesignBlock, type-30 = BattlePlanBlock, type-34 = ResearchChangeBlock.

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::{
    blocks::{
        decode_type13, decode_type16, decode_type26, decode_type30, decode_type6, decode_waypoint,
        BattlePlanRecord, DesignRecord, FleetRecord, PlanetRecord, WaypointRecord,
    },
    messages::{decode_events_block, EventsRecord},
    race::Race,
    records::{game_year, parse_file},
};

// ── Output types (player-centric turn view) ────────────────────────────────────

/// Player-visible state extracted from the .m1 file.
#[derive(Debug, Serialize)]
struct PlayerState {
    /// Homeworld planet index (type-6 bytes 8-9 LE uint16, confirmed: IT=73, JOAT=52).
    homeworld_planet_idx: Option<u16>,

    /// Number of planets owned by this player (type-6 bytes 2-3 LE uint16).
    planet_count: Option<u16>,

    // ── Tech levels from type-6 bytes 26-31 ──
    tech_energy: Option<u8>,
    tech_weapons: Option<u8>,
    tech_propulsion: Option<u8>,
    tech_construction: Option<u8>,
    tech_electronics: Option<u8>,
    tech_biology: Option<u8>,

    /// Race design parameters from type-6 bytes 16-81 plus name/plural at bytes 112+.
    #[serde(skip_serializing_if = "Option::is_none")]
    race: Option<Race>,

    /// One entry per type-30 BattlePlanBlock record.
    battle_plans: Vec<BattlePlanRecord>,
}

/// Complete turn state extracted from one .m1 file.
#[derive(Debug, Serialize)]
struct TurnState {
    /// Game year (type-8 header payload bytes 10-11 as LE uint16, + 2400).
    year: u32,
    player: PlayerState,
    planets: Vec<PlanetRecord>,
    /// Fleet records (type-16 FleetBlock).
    fleets: Vec<FleetRecord>,
    /// Waypoint records (type-19 TaskWaypointBlock and type-20 WaypointBlock), in file order.
    waypoints: Vec<WaypointRecord>,
    /// Ship and starbase designs (type-26 DesignBlock, full-design variant only).
    designs: Vec<DesignRecord>,
    /// EventsBlock (type-12) records — partial decode.
    events: Vec<EventsRecord>,
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: m1_to_json <file.m1>");
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

    // Active player index is encoded in the filename: Game.mN  →  N-1.
    // We use it to disambiguate which type-6 PlayerBlock is the active
    // player's own (when the file holds blocks for multiple players).
    // Falls back to None if the filename doesn't match the .mN pattern,
    // in which case the parser uses the first ≥32-byte block.
    let active_player_id: Option<u8> = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|s| s.strip_prefix('m'))
        .and_then(|s| s.parse::<u8>().ok())
        .map(|n| n.saturating_sub(1));

    let year = game_year(&records).unwrap_or(2400);
    let mut homeworld_planet_idx: Option<u16> = None;
    let mut planet_count: Option<u16> = None;
    let mut techs: [Option<u8>; 6] = [None; 6];
    let mut race: Option<Race> = None;
    let mut battle_plans: Vec<BattlePlanRecord> = Vec::new();
    let mut planets: Vec<PlanetRecord> = Vec::new();
    let mut fleets: Vec<FleetRecord> = Vec::new();
    let mut waypoints: Vec<WaypointRecord> = Vec::new();
    let mut designs: Vec<DesignRecord> = Vec::new();
    let mut events: Vec<EventsRecord> = Vec::new();

    for rec in &records {
        match rec.rtype {
            12 => {
                events.push(decode_events_block(&rec.payload));
            }
            6 => {
                // A .mN file may contain one type-6 PlayerBlock per known
                // player (own + any whose race has been learned via
                // espionage / scoreboard).  Each block's first byte is the
                // 0-indexed player ID.  Active player's own block also has
                // payload ≥ 32 bytes (tech at b26-31, race spec at b16-81).
                if rec.payload.is_empty() || rec.payload.len() < 32 {
                    continue;
                }
                let block_pid = rec.payload[0];
                let take = match active_player_id {
                    Some(active) => block_pid == active,
                    None => homeworld_planet_idx.is_none(),
                };
                if take {
                    let pb = decode_type6(&rec.payload);
                    homeworld_planet_idx = pb.homeworld_planet_idx;
                    planet_count = pb.planet_count;
                    techs = pb.techs;
                    race = pb.race;
                }
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
            19 => {
                if let Some(wp) = decode_waypoint(&rec.payload, true) {
                    waypoints.push(wp);
                }
            }
            20 => {
                if let Some(wp) = decode_waypoint(&rec.payload, false) {
                    waypoints.push(wp);
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
            _ => {}
        }
    }

    let turn = TurnState {
        year,
        player: PlayerState {
            homeworld_planet_idx,
            planet_count,
            tech_energy: techs[0],
            tech_weapons: techs[1],
            tech_propulsion: techs[2],
            tech_construction: techs[3],
            tech_electronics: techs[4],
            tech_biology: techs[5],
            race,
            battle_plans,
        },
        planets,
        fleets,
        waypoints,
        designs,
        events,
    };

    let json = serde_json::to_string_pretty(&turn).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
