// m1_to_json — read a Stars! .m1 player turn file and emit turn JSON on stdout.
//
// Usage:  m1_to_json <file.m1>
//
// Output is a JSON object with the following top-level keys:
//   "year"    — game year decoded from type-8 file header (turn + 2400)
//   "player"  — homeworld coords + type-30 records (see note below)
//   "planets" — list of planets visible to the player (from type-13 PlanetBlock)
//   "fleets"  — fleet positions (from type-16 FleetBlock)
//   "waypoints" — waypoint records (from type-20 WaypointBlock)
//
// This is the primary oracle comparison tool for Phase R2–R5 research.
// It is built from the confirmed field offsets in:
//   stars-reborn-research/docs/findings/binary_file_format.rst
//
// Record type name corrections (from XyliGUN's block type registry, 2011):
//   type-16 = FleetBlock      (previously misidentified as "ship design")
//   type-20 = WaypointBlock   (previously misidentified as "fleet")
//   type-26 = DesignBlock     (previously "fleet/design aggregate") — confirmed
//   type-30 = BattlePlanBlock (previously assumed "research data")
//   type-34 = ResearchChangeBlock — this is the actual research record
//
// NOTE on type-30 ("player.research" output):
//   Field layout at bytes 0 (field_id) and 1 (level) was mapped against
//   tutorial data and appeared to encode tech fields 0-5 with level values.
//   XyliGUN identifies type-30 as BattlePlanBlock; current tech levels are
//   more likely encoded in type-6 (PlayerBlock, 131 bytes, mostly unexplored).
//   The "research" output here is kept for continuity but needs re-verification.
//
// Known limitations / TODOs
// -------------------------------------------------------------------------
// - Planet record (type-13): b14-15, b18-19, b20-21 offsets TBD (surface
//   ironium/germanium, population — ordering unknown).
// - FleetBlock (type-16): fleet name, ship count, design reference UNKNOWN.
// - Type-6 PlayerBlock (131 bytes): tech levels, race traits, other player
//   data mostly unexplored.
// - Type-30 "research" vs BattlePlanBlock: needs re-investigation.

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::records::parse_file;

// ── Output types ─────────────────────────────────────────────────────────────

/// One tech field from a type-30 research record.
#[derive(Debug, Serialize)]
struct TechField {
    /// Raw field id byte (0–5 expected).  Mapping to name unconfirmed; see
    /// binary_file_format.rst type-30 notes.
    field_id: u8,
    level: u8,
    // TODO: resources_to_next_level (bytes unknown in type-30 payload)
}

/// Player-visible state extracted from the .m1 file.
#[derive(Debug, Serialize)]
struct PlayerState {
    /// Y coordinate of the homeworld (type-6 bytes 3-4 LE int16, confirmed).
    homeworld_y: Option<i16>,
    /// X coordinate of the homeworld (type-16 bytes 8-9 LE int16, confirmed).
    homeworld_x: Option<i16>,
    /// One entry per type-30 record.
    research: Vec<TechField>,
}

/// One planet record (type-13).
#[derive(Debug, Serialize)]
struct PlanetRecord {
    /// Planet index word (bytes 0-1 LE uint16): low 10 bits = 0-based planet
    /// index, high 6 bits = owner flags (62=uninhabited, 0=player1, 2=player2).
    index: u16,
    /// True if this is a colonised planet (34-byte record vs 11-byte uninhabited).
    colonized: bool,

    // ── Confirmed fields (both uninhabited and colonised) ─────────────────
    /// Ironium   mineral concentration (0–100) — byte 5, confirmed.
    conc_ironium: Option<u8>,
    /// Boranium  mineral concentration (0–100) — byte 6, confirmed.
    conc_boranium: Option<u8>,
    /// Germanium mineral concentration (0–100) — byte 7, confirmed.
    conc_germanium: Option<u8>,
    /// Gravity index (0–100; look up in GRAV_CENTI table) — byte 8, confirmed.
    gravity: Option<u8>,
    /// Temperature index (0–100; temp°C = (idx − 50) × 4) — byte 9, confirmed.
    temperature: Option<u8>,
    /// Radiation (0–100 mR/yr) — byte 10, confirmed.
    radiation: Option<u8>,

    // ── Confirmed colonised-only fields ───────────────────────────────────
    /// Surface boranium in kT (bytes 16-17 LE int16, confirmed).
    surface_boranium: Option<i16>,
    /// Number of mines (bytes 24-25 LE int16, confirmed).
    mines: Option<i16>,

    // ── TODO: resolve remaining colonised offsets ─────────────────────────
    /// Surface ironium in kT.  Likely b14-15 or b18-19 — offset TBD.
    surface_ironium: Option<i16>,
    /// Surface germanium in kT.  Likely b14-15 or b18-19 — offset TBD.
    surface_germanium: Option<i16>,
    /// Colony population.  Likely b20-21 (kilo-colonists × 100) — offset TBD.
    population: Option<u32>,
    /// Number of factories.  Offset unknown.
    factories: Option<i16>,
}

/// One waypoint record (type-20, WaypointBlock).
///
/// Previously misidentified as a "fleet record"; XyliGUN's type registry
/// identifies type-20 as WaypointBlock.  At turn 1 the initial waypoint for
/// the homeworld fleet is "orbit homeworld," so waypoint X = homeworld X and
/// planet_index = homeworld planet index.
#[derive(Debug, Serialize)]
struct WaypointRecord {
    /// Waypoint X coordinate (bytes 0-1 LE int16, confirmed).
    x: i16,
    /// Target planet index, or -1 if the waypoint is in deep space
    /// (bytes 4-5 LE int16, confirmed).
    planet_index: i16,
    // TODO: waypoint task type, repeat orders flag (offsets unknown)
}

/// Complete turn-1 state extracted from one .m1 file.
#[derive(Debug, Serialize)]
struct TurnState {
    /// Game year (type-8 header payload bytes 10-11 as LE uint16, + 2400).
    year: u32,
    player: PlayerState,
    planets: Vec<PlanetRecord>,
    /// Waypoint records (type-20 WaypointBlock). Previously labelled "fleets"
    /// but XyliGUN's registry identifies type-20 as WaypointBlock.
    waypoints: Vec<WaypointRecord>,
}

// ── Record decoders ───────────────────────────────────────────────────────────

fn read_i16_le(p: &[u8], off: usize) -> Option<i16> {
    if off + 2 <= p.len() {
        Some(i16::from_le_bytes([p[off], p[off + 1]]))
    } else {
        None
    }
}

fn decode_type6(p: &[u8]) -> Option<i16> {
    // bytes 3-4: homeworld Y coordinate (LE int16), confirmed.
    read_i16_le(p, 3)
}

fn decode_type13(p: &[u8]) -> Option<PlanetRecord> {
    if p.len() < 2 {
        return None;
    }
    let index = u16::from_le_bytes([p[0], p[1]]);
    // Uninhabited = 11 bytes; colonised = 34 bytes.
    let colonized = p.len() >= 34;

    Some(PlanetRecord {
        index,
        colonized,
        // b5-b10 are present in both uninhabited (11 bytes) and colonised (34 bytes).
        conc_ironium:      if p.len() > 5  { Some(p[5])  } else { None },
        conc_boranium:     if p.len() > 6  { Some(p[6])  } else { None },
        conc_germanium:    if p.len() > 7  { Some(p[7])  } else { None },
        gravity:           if p.len() > 8  { Some(p[8])  } else { None },
        temperature:       if p.len() > 9  { Some(p[9])  } else { None },
        radiation:         if p.len() > 10 { Some(p[10]) } else { None },
        // Colonised-only fields.
        surface_boranium:  if colonized { read_i16_le(p, 16) } else { None },
        mines:             if colonized { read_i16_le(p, 24) } else { None },
        // TODO: resolve b14-15 (surf_iron or surf_germ?), b18-19, b20-21 (population?)
        surface_ironium:   None,
        surface_germanium: None,
        population:        None,
        factories:         None,
    })
}

fn decode_type16(p: &[u8]) -> Option<i16> {
    // FleetBlock: bytes 8-9 = fleet X coordinate (LE int16).
    // At turn 1, the homeworld fleet has not moved, so fleet X = homeworld X.
    read_i16_le(p, 8)
}

fn decode_type20(p: &[u8]) -> Option<WaypointRecord> {
    // WaypointBlock: bytes 0-1 = waypoint X (LE int16); bytes 4-5 = target
    // planet index (LE int16).  At turn 1 the initial waypoint is "orbit
    // homeworld," so waypoint X = homeworld X and planet_index = homeworld idx.
    if p.len() < 6 {
        return None;
    }
    Some(WaypointRecord {
        x:            i16::from_le_bytes([p[0], p[1]]),
        planet_index: i16::from_le_bytes([p[4], p[5]]),
    })
}

fn decode_type30(p: &[u8]) -> Option<TechField> {
    if p.len() < 2 {
        return None;
    }
    Some(TechField {
        field_id: p[0],
        level:    p[1],
    })
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

    let mut year: u32 = 2400; // overwritten when type-8 header is seen
    let mut homeworld_y: Option<i16> = None;
    let mut homeworld_x: Option<i16> = None;
    let mut research: Vec<TechField> = Vec::new();
    let mut planets: Vec<PlanetRecord> = Vec::new();
    let mut waypoints: Vec<WaypointRecord> = Vec::new();

    for rec in &records {
        match rec.rtype {
            // Type-8 plaintext file header: payload bytes 10-11 (LE uint16) hold
            // the turn offset; year = turn_offset + 2400 (confirmed via starstat.pl).
            8 => {
                if rec.payload.len() >= 12 {
                    let turn_raw = u16::from_le_bytes([rec.payload[10], rec.payload[11]]);
                    year = 2400 + turn_raw as u32;
                }
            }
            6 => {
                if homeworld_y.is_none() {
                    homeworld_y = decode_type6(&rec.payload);
                }
            }
            13 => {
                if let Some(planet) = decode_type13(&rec.payload) {
                    planets.push(planet);
                }
            }
            16 => {
                // FleetBlock: extract fleet X as proxy for homeworld X (valid at turn 1).
                if homeworld_x.is_none() {
                    homeworld_x = decode_type16(&rec.payload);
                }
            }
            20 => {
                // WaypointBlock (previously misidentified as fleet record).
                if let Some(wp) = decode_type20(&rec.payload) {
                    waypoints.push(wp);
                }
            }
            30 => {
                // BattlePlanBlock per XyliGUN's registry; kept as "research" for
                // now but field mapping needs re-investigation against type-6.
                if let Some(tf) = decode_type30(&rec.payload) {
                    research.push(tf);
                }
            }
            _ => {}
        }
    }

    let turn = TurnState {
        year,
        player: PlayerState { homeworld_x, homeworld_y, research },
        planets,
        waypoints,
    };

    let json = serde_json::to_string_pretty(&turn).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
