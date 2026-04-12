// m1_to_json — read a Stars! .m1 player turn file and emit turn JSON on stdout.
//
// Usage:  m1_to_json <file.m1>
//
// Output is a JSON object with the following top-level keys:
//   "year"    — game year decoded from type-8 file header (turn + 2400)
//   "player"  — player state from type-6 PlayerBlock (tech levels, homeworld idx)
//   "planets" — list of planets visible to the player (from type-13 PlanetBlock)
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
// Confirmed type-6 field layout (from Structure6.xml + oracle experiments):
//   b0    : PlayerID
//   b1    : ShipSlotsUsed
//   b2-3  : PlanetCount (LE uint16)
//   b4-5  : FleetAndStarBaseDesignCount (LE uint16; 12 bits fleet + 4 bits starbase)
//   b6    : Logo/FullRace flags
//   b7    : Unknown (always 1)
//   b8-9  : Homeworld planet index (LE uint16) — confirmed IT=73, JOAT=52
//   b10-11: Homeworld rank (LE uint16)
//   b12-15: PasswordHash (4 bytes)
//   b16   : CentreGravity hab setting
//   b17   : CentreTemperature hab setting
//   b18   : CentreRadiation hab setting
//   b19   : LowGravity
//   b20   : LowTemperature
//   b21   : LowRadiation
//   b22   : HighGravity
//   b23   : HighTemperature
//   b24   : HighRadiation
//   b25   : GrowthRate
//   b26   : EnergyLevel — confirmed (IT=0, JOAT=3)
//   b27   : WeaponsLevel — confirmed (IT=0, JOAT=3)
//   b28   : PropulsionLevel — confirmed (IT=5, JOAT=3)
//   b29   : ConstructionLevel — confirmed (IT=5, JOAT=3)
//   b30   : ElectronicsLevel — confirmed (IT=0, JOAT=3)
//   b31   : BiologyLevel — confirmed (IT=0, JOAT=3)
//
// Known limitations / TODOs
// -------------------------------------------------------------------------
// - Planet record (type-13): uses simplified fixed-offset decoding that only
//   works for non-depleted, non-terraformed planets. Full variable-length
//   parsing (DepletionLength + SurfaceLength bytes) not yet implemented.
//   Surface minerals at b14-19 and population at b20+ confirmed for standard
//   (non-depleted) case per Structure13.xml.
// - FleetBlock (type-16): fleet name, ship count, design reference UNKNOWN.
// - Type-30 BattlePlanBlock: full field layout unknown; renamed from "research".

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::records::parse_file;

// ── Output types ─────────────────────────────────────────────────────────────

/// One battle plan record (type-30 BattlePlanBlock).
///
/// Previously labelled "TechField" / "research" because field_id values 0-5
/// superficially resembled tech field indices.  XyliGUN's type registry and
/// the discovery of actual tech levels in type-6 bytes 26-31 confirm that
/// type-30 = BattlePlanBlock.  The field_id values (0, 16, 32, 48, 64 — all
/// multiples of 16) are consistent with battle plan ID encoding, not tech
/// fields (which would be 0-5).  Full layout still unknown.
#[derive(Debug, Serialize)]
struct BattlePlanRecord {
    /// Battle plan ID byte (multiples of 16: 0,16,32,48,64 for 5 default plans).
    plan_id: u8,
    /// Aggressiveness / order type (0-4: 0=disengage, 4=maximise damage).
    order_type: u8,
    // TODO: full battle plan field layout (target, primary target, etc.)
}

/// Player-visible state extracted from the .m1 file.
#[derive(Debug, Serialize)]
struct PlayerState {
    /// Homeworld planet index (type-6 bytes 8-9 LE uint16, confirmed: IT=73, JOAT=52).
    homeworld_planet_idx: Option<u16>,
    /// X coordinate of the homeworld fleet (type-16 bytes 8-9 LE int16, valid at turn 1).
    homeworld_x: Option<i16>,

    // ── Tech levels from type-6 bytes 26-31 (confirmed against oracle experiments) ──
    /// Energy tech level — type-6 byte 26 (confirmed: IT=0, JOAT=3).
    tech_energy: Option<u8>,
    /// Weapons tech level — type-6 byte 27 (confirmed: IT=0, JOAT=3).
    tech_weapons: Option<u8>,
    /// Propulsion tech level — type-6 byte 28 (confirmed: IT=5, JOAT=3).
    tech_propulsion: Option<u8>,
    /// Construction tech level — type-6 byte 29 (confirmed: IT=5, JOAT=3).
    tech_construction: Option<u8>,
    /// Electronics tech level — type-6 byte 30 (confirmed: IT=0, JOAT=3).
    tech_electronics: Option<u8>,
    /// Biology tech level — type-6 byte 31 (confirmed: IT=0, JOAT=3).
    tech_biology: Option<u8>,

    /// One entry per type-30 BattlePlanBlock record.
    battle_plans: Vec<BattlePlanRecord>,
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
    // NOTE: These fixed offsets assume SurfaceLength byte = 0xAA (2 bytes each
    // for iron/boran/germ/pop) and no depletion bytes. This holds for standard
    // non-depleted planets. Full variable-length parsing (per Structure13.xml)
    // not yet implemented.
    /// Surface ironium in kT (bytes 14-15 LE uint16, confirmed per Structure13.xml).
    surface_ironium: Option<u16>,
    /// Surface boranium in kT (bytes 16-17 LE uint16, confirmed).
    surface_boranium: Option<u16>,
    /// Surface germanium in kT (bytes 18-19 LE uint16, confirmed per Structure13.xml).
    surface_germanium: Option<u16>,
    /// Colony population in units of 100 colonists (bytes 20-21 LE uint16).
    /// Actual colonists = value × 100.  Width may be 1 byte if population < 256.
    population: Option<u32>,
    /// Number of mines (bytes 24-25 LE uint16, confirmed).
    mines: Option<u16>,
    /// Number of factories — offset unconfirmed (follows mines in Installations block).
    factories: Option<u16>,
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
    /// Waypoint records (type-20 WaypointBlock).
    waypoints: Vec<WaypointRecord>,
}

// ── Record decoders ───────────────────────────────────────────────────────────

fn read_u16_le(p: &[u8], off: usize) -> Option<u16> {
    if off + 2 <= p.len() {
        Some(u16::from_le_bytes([p[off], p[off + 1]]))
    } else {
        None
    }
}

fn read_i16_le(p: &[u8], off: usize) -> Option<i16> {
    read_u16_le(p, off).map(|v| v as i16)
}

/// Decode a type-6 PlayerBlock into player state.
///
/// Confirmed layout (from Structure6.xml + oracle experiments):
///   b8-9  : Homeworld planet index (LE uint16)
///   b26   : EnergyLevel       (IT=0, JOAT=3)
///   b27   : WeaponsLevel      (IT=0, JOAT=3)
///   b28   : PropulsionLevel   (IT=5, JOAT=3)
///   b29   : ConstructionLevel (IT=5, JOAT=3)
///   b30   : ElectronicsLevel  (IT=0, JOAT=3)
///   b31   : BiologyLevel      (IT=0, JOAT=3)
fn decode_type6(p: &[u8]) -> (Option<u16>, [Option<u8>; 6]) {
    let hw_idx = read_u16_le(p, 8);
    let techs = [
        if p.len() > 26 { Some(p[26]) } else { None }, // energy
        if p.len() > 27 { Some(p[27]) } else { None }, // weapons
        if p.len() > 28 { Some(p[28]) } else { None }, // propulsion
        if p.len() > 29 { Some(p[29]) } else { None }, // construction
        if p.len() > 30 { Some(p[30]) } else { None }, // electronics
        if p.len() > 31 { Some(p[31]) } else { None }, // biology
    ];
    (hw_idx, techs)
}

fn decode_type13(p: &[u8]) -> Option<PlanetRecord> {
    if p.len() < 2 {
        return None;
    }
    let index = u16::from_le_bytes([p[0], p[1]]);
    // Colonised records are longer than uninhabited (11-byte) records.
    // Exact length varies with SurfaceLength flags + depletion bytes; the
    // surface mineral fields below assume the common non-depleted case where
    // the SurfaceLength byte (b13) encodes 2 bytes for each mineral and
    // population (0xAA or 0x6A).  Full variable-length parsing is TODO.
    let colonized = p.len() > 11;

    Some(PlanetRecord {
        index,
        colonized,
        // b5-b10 present in both uninhabited and colonised records.
        conc_ironium:      if p.len() > 5  { Some(p[5])  } else { None },
        conc_boranium:     if p.len() > 6  { Some(p[6])  } else { None },
        conc_germanium:    if p.len() > 7  { Some(p[7])  } else { None },
        gravity:           if p.len() > 8  { Some(p[8])  } else { None },
        temperature:       if p.len() > 9  { Some(p[9])  } else { None },
        radiation:         if p.len() > 10 { Some(p[10]) } else { None },
        // Colonised-only surface mineral fields (confirmed offsets for non-depleted planets).
        // Offset layout: b13=SurfaceLength, then 2 bytes each: iron, boran, germ, pop.
        surface_ironium:   if colonized { read_u16_le(p, 14) } else { None },
        surface_boranium:  if colonized { read_u16_le(p, 16) } else { None },
        surface_germanium: if colonized { read_u16_le(p, 18) } else { None },
        population:        if colonized { read_u16_le(p, 20).map(|v| v as u32) } else { None },
        // TODO: mines and factories are in the variable-offset Installations block that
        // follows the surface minerals.  Exact start byte depends on SurfaceLength (b13),
        // DepletionLength (b4), and PlanetInfo flags.  Not yet decoded.
        mines:     None,
        factories: None,
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

fn decode_type30(p: &[u8]) -> Option<BattlePlanRecord> {
    if p.len() < 2 {
        return None;
    }
    Some(BattlePlanRecord {
        plan_id:    p[0],
        order_type: p[1],
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
    let mut homeworld_planet_idx: Option<u16> = None;
    let mut homeworld_x: Option<i16> = None;
    let mut techs: [Option<u8>; 6] = [None; 6];
    let mut battle_plans: Vec<BattlePlanRecord> = Vec::new();
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
                if homeworld_planet_idx.is_none() {
                    let (hw_idx, t) = decode_type6(&rec.payload);
                    homeworld_planet_idx = hw_idx;
                    techs = t;
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
                if let Some(wp) = decode_type20(&rec.payload) {
                    waypoints.push(wp);
                }
            }
            30 => {
                // BattlePlanBlock per XyliGUN's registry.
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
            homeworld_x,
            tech_energy:       techs[0],
            tech_weapons:      techs[1],
            tech_propulsion:   techs[2],
            tech_construction: techs[3],
            tech_electronics:  techs[4],
            tech_biology:      techs[5],
            battle_plans,
        },
        planets,
        waypoints,
    };

    let json = serde_json::to_string_pretty(&turn).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
