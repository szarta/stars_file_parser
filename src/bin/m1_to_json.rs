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
//
// This is the primary oracle comparison tool for Phase R2–R5 research.
// It is built from the confirmed field offsets in:
//   stars-reborn-research/docs/findings/binary_file_format.rst
//
// Record type name corrections (from XyliGUN's block type registry, 2011):
//   type-16 = FleetBlock         (previously misidentified as "ship design")
//   type-19 = TaskWaypointBlock  (waypoint with task; same header as type-20, +10 task bytes)
//   type-20 = WaypointBlock      (plain waypoint / goto / orbit)
//   type-26 = DesignBlock
//   type-30 = BattlePlanBlock    (previously assumed "research data")
//   type-34 = ResearchChangeBlock
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
// - FleetBlock (type-16): fleet name encoding unknown; b15 high bit and
//   b17-18 encoding may change when cargo is present.
// - Type-19 task payload (b8-17): Transport fully confirmed (exp4); payload
//   decoded into TransportPayload struct.  Colonize payload is all zero.
//   Other task types (Remote Mining, Patrol, etc.) not yet decoded.

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::{race::{race_from_payload, Race}, records::parse_file};

// ── Output types ─────────────────────────────────────────────────────────────

/// One battle plan record (type-30 BattlePlanBlock).
///
/// Confirmed field layout (from user-provided default plan settings + oracle cross-reference):
///   b0  : plan_id = plan_index × 16 (0,16,32,48,64,80 for 6 plans)
///   b1  : tactic (0=Disengage, 1=Disengage if challenged, 3=Max net damage, 4=Max damage ratio)
///   b2  : targets packed byte: high nybble = secondary target, low nybble = primary target
///          Target codes: 0=None, 1=Any, 2=Starbase, 3=Armed Ships, 4=Bombers/Freighters,
///                        5=Unarmed Ships, 6=Fuel Transports, 7=Freighters
///   b3  : 0x02 for all default plans = "Everyone" (Attack Who setting)
///   b4  : remaining byte count (total_len - 5)
///   b5+ : unknown (possibly plan name in packed encoding; length varies by plan)
///
/// Default plan verification (all 5 confirmed from oracle exp 2.2):
///   Default(0):       tactic=4, pri=3(Armed), sec=1(Any)      ✓
///   Kill Starbase(1): tactic=4, pri=2(Starbase), sec=3(Armed) ✓
///   Max-Defense(2):   tactic=3, pri=3(Armed), sec=4(Bombers)  ✓
///   Sniper(3):        tactic=1, pri=5(Unarmed), sec=0(None)   ✓
///   Chicken(4):       tactic=0, pri=1(Any), sec=0(None)       ✓
///   Test(5) exp3:     tactic=3, pri=7(Freighters), sec=6(FuelTransports) ✓
#[derive(Debug, Serialize)]
struct BattlePlanRecord {
    /// Plan index × 16 (0, 16, 32, 48, 64, 80 for plans 0-5).
    plan_id: u8,
    /// Tactic: 0=Disengage, 1=Disengage if challenged, 3=Max net damage, 4=Max damage ratio.
    tactic: u8,
    /// Primary target code (low nybble of b2):
    /// 1=Any, 2=Starbase, 3=Armed Ships, 4=Bombers/Freighters,
    /// 5=Unarmed Ships, 6=Fuel Transports, 7=Freighters.
    primary_target: u8,
    /// Secondary target code (high nybble of b2):
    /// 0=None, 1=Any, 3=Armed Ships, 4=Bombers/Freighters, 6=Fuel Transports, 7=Freighters.
    secondary_target: u8,
    /// Attack Who: 0x02 = Everyone for all default plans.
    attack_who: u8,
}

/// Per-resource cargo operation within a Transport task.
///
/// Encoded as two consecutive bytes in the type-19 payload (confirmed exp4):
///   byte_lo = amount & 0xFF
///   byte_hi = (action << 4) | ((amount >> 8) & 0xF)
///
/// Action codes (0=confirmed absent, 3=confirmed; 1,2,4,5 hypothesised):
///   0 = No change, 1 = Unload All, 2 = Load All, 3 = Load Exactly,
///   4 = Unload Exactly, 5 = Fill Up To
#[derive(Debug, Serialize)]
struct ResourceOp {
    /// Amount in kT (0–4095).
    amount: u16,
    /// Action code: 0=No change, 1=Unload All, 2=Load All, 3=Load Exactly,
    /// 4=Unload Exactly, 5=Fill Up To.
    action: u8,
}

/// Transport task payload decoded from type-19 bytes b8-17 (10 bytes, 5 × 2).
///
/// Confirmed from exp4 (Teamster→Silicon: Load Exactly 10 Iron, 20 Boran, 30 Germ):
///   b8-9:   ironium   (10kT, action=3=Load Exactly) ✓
///   b10-11: boranium  (20kT, action=3) ✓
///   b12-13: germanium (30kT, action=3) ✓
///   b14-15: colonists (0,    action=0) ✓
///   b16-17: fuel      (0,    action=0) ✓
#[derive(Debug, Serialize)]
struct TransportPayload {
    ironium:   ResourceOp,
    boranium:  ResourceOp,
    germanium: ResourceOp,
    /// Colonists, in units of 100 (same as population elsewhere in the format).
    colonists: ResourceOp,
    fuel:      ResourceOp,
}

/// Player-visible state extracted from the .m1 file.
#[derive(Debug, Serialize)]
struct PlayerState {
    /// Homeworld planet index (type-6 bytes 8-9 LE uint16, confirmed: IT=73, JOAT=52).
    homeworld_planet_idx: Option<u16>,

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

    /// Race design parameters from type-6 bytes 16-81.
    ///
    /// Bytes 16-81 have IDENTICAL layout in both .r1 and .m type-6 payloads
    /// (confirmed 2026-04-13, R0.9).  name/plural_name/icon_index are left
    /// empty/zero because their encoding differs from .r1 files.
    #[serde(skip_serializing_if = "Option::is_none")]
    race: Option<Race>,

    /// One entry per type-30 BattlePlanBlock record.
    battle_plans: Vec<BattlePlanRecord>,
}

/// One planet record (type-13 PlanetBlock).
///
/// The record is variable-length (11 bytes uninhabited; colonised length depends
/// on SurfaceLength byte b13 and PlanetInfo flags b2-3).  Surface minerals and
/// the Installations block are decoded dynamically; only non-depleted, non-terraformed
/// planets are fully supported in this simplified decoder.
#[derive(Debug, Serialize)]
struct PlanetRecord {
    /// 0-based planet index (low 11 bits of the PlanetIDAndOwnerID word).
    planet_index: u16,
    /// Owner ID (high 5 bits of PlanetIDAndOwnerID word; 31 = nobody/uninhabited,
    /// 0 = player 1, 1 = player 2, …).
    owner_id: u8,
    /// True if this is a colonised planet (record longer than 11 bytes).
    colonized: bool,

    // ── Present in both uninhabited and colonised records ─────────────────
    /// Ironium   concentration (0–100) — b5.
    conc_ironium: Option<u8>,
    /// Boranium  concentration (0–100) — b6.
    conc_boranium: Option<u8>,
    /// Germanium concentration (0–100) — b7.
    conc_germanium: Option<u8>,
    /// Gravity index (0–100) — b8.
    gravity: Option<u8>,
    /// Temperature index (0–100; temp°C = (idx − 50) × 4) — b9.
    temperature: Option<u8>,
    /// Radiation (0–100 mR/yr) — b10.
    radiation: Option<u8>,

    // ── Colonised-only: surface minerals and population ───────────────────
    // Offsets are computed from SurfaceLength byte (b13); valid for non-depleted,
    // non-terraformed planets where b4=0 and Terraformed flag=0.
    /// Surface ironium in kT (LE uint, width from SurfaceLength bits 0-1).
    surface_ironium: Option<u32>,
    /// Surface boranium in kT (LE uint, width from SurfaceLength bits 2-3).
    surface_boranium: Option<u32>,
    /// Surface germanium in kT (LE uint, width from SurfaceLength bits 4-5).
    surface_germanium: Option<u32>,
    /// Colony population in units of 100 colonists (LE uint, width from SurfaceLength bits 6-7).
    population: Option<u32>,

    // ── Colonised-only: Installations block (b22-24 for standard 0x6A layout) ──
    // Decoded from the 3-byte Installations field: Mines in bits 0-11, Factories bits 12-23.
    // Confirmed: all three oracle experiments show 10 mines and 10 factories at turn 1.
    mines: Option<u16>,
    factories: Option<u16>,
    /// Defenses — low 12 bits of the 2-byte Defenses field.  Confirmed: 10 at turn 1.
    defenses: Option<u16>,
}

/// One fleet record (type-16 FleetBlock).
///
/// Confirmed field layout (from oracle experiments exp2.2 and exp3):
///   b0-1  : fleet index (LE uint16)
///   b6-7  : current orbit planet index (LE uint16); 65535 = en route (deep space)
///   b8-9  : fleet X coordinate (LE int16)
///   b10-11: fleet Y coordinate (LE int16) — confirmed: Sulfur Y=1220, Godel Y=1331
///   b12-13: design bitmask (bit i set → design slot i present in fleet)
///   b14-15: number of stacks (b14 = actual count; b15 high bit may indicate cargo)
///   b16   : total ship count
///   b17-18: total fleet mass in kT (LE int16; high bits may be set when cargo present)
///
///   Trailing bytes (confirmed from exp2.2 vs exp3 differential):
///     22-byte records: b19-20=0, b21=waypoint_count
///     23-byte records: b19-20=0, b21=battle_plan_idx (0-based), b22=waypoint_count
///   Note: records grow from 22→23 bytes when fleet has cargo or multiple ships.
#[derive(Debug, Serialize)]
struct FleetRecord {
    fleet_index: u16,
    /// Planet currently orbited (LE uint16); 65535 = en route / deep space.
    orbit_planet_idx: u16,
    /// True when orbit_planet_idx == 65535 (fleet is moving between planets).
    en_route: bool,
    x: i16,
    y: i16,
    /// Bitmask of design slots used (bit 0 = slot 0, bit 1 = slot 1, …).
    design_bitmask: u16,
    /// Number of distinct design stacks in this fleet.
    num_stacks: u16,
    /// Total number of ships across all stacks.
    ship_count: u8,
    /// Total fleet mass in kT (LE int16; high bits may encode cargo info when cargo present).
    total_mass_kt: i16,
    /// 0-based index into player's battle plan list (present only in 23-byte records).
    /// 0=Default, 1=Kill Starbase, 2=Max-Defense, 3=Sniper, 4=Chicken, 5+=custom.
    battle_plan_idx: Option<u8>,
    /// Number of waypoints for this fleet (including current-position waypoint when en route).
    waypoint_count: u8,
}

/// One waypoint record (type-19 TaskWaypointBlock or type-20 WaypointBlock).
///
/// Both types share the same 8-byte header layout:
///   b0-1: waypoint X (LE int16)
///   b2-3: waypoint Y (LE int16)
///   b4-5: target planet index (LE int16; 0 = deep space / no planet)
///   b6  : (warp_speed << 4) | task_type
///           warp_speed: high nybble (0=orbit/stopped, 1-10=warp N) — confirmed at warp 4 and 6
///           task_type:  low nybble (0=none, 1=Transport, 2=Colonize, 3=Remote Mining,
///                        4=Merge with Fleet, 5=Scrap Fleet, 6=Lay Mine Field,
///                        7=Patrol, 8=Route, 9=Transfer Fleet)
///   b7  : unknown (bit 2 set = this is the "current position" waypoint for en-route fleets;
///                  high nybble varies per fleet type; not battle plan assignment)
///
/// Type-19 has an additional 10 bytes (b8-17) for task parameters.
/// For task_type=1 (Transport) these are decoded into `transport_payload`.
/// For task_type=2 (Colonize) all 10 bytes are zero.
/// Other task types are not yet decoded.
#[derive(Debug, Serialize)]
struct WaypointRecord {
    /// Waypoint X coordinate (bytes 0-1 LE int16).
    x: i16,
    /// Waypoint Y coordinate (bytes 2-3 LE int16).
    y: i16,
    /// Target planet index (bytes 4-5 LE int16; 0 = deep space / en-route current position).
    planet_index: i16,
    /// Warp speed: 0 = orbit/stationary, 1-10 = warp N (b6 >> 4).
    warp_speed: u8,
    /// Waypoint task type (b6 & 0xF): 0=none, 1=Transport, 2=Colonize, 3=Remote Mining,
    /// 4=Merge with Fleet, 5=Scrap Fleet, 6=Lay Mine Field, 7=Patrol, 8=Route, 9=Transfer Fleet.
    task_type: u8,
    /// True if this waypoint represents the fleet's current position (bit 2 of b7 set).
    /// Always the first waypoint for en-route fleets; warp_speed and task_type are 0 for it.
    is_current_position: bool,
    /// True if decoded from a type-19 TaskWaypointBlock (has task payload at b8-17).
    /// False if decoded from a type-20 WaypointBlock (plain goto / orbit).
    has_task_payload: bool,
    /// Present when task_type=1 (Transport) and has_task_payload=true.
    /// Decoded from type-19 b8-17 (5 × 2-byte resource operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    transport_payload: Option<TransportPayload>,
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
    /// Each fleet's waypoints follow its type-16 record; the first waypoint for an en-route
    /// fleet is the current position (is_current_position=true, warp_speed=0).
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

#[allow(dead_code)]
fn read_i16_le(p: &[u8], off: usize) -> Option<i16> {
    read_u16_le(p, off).map(|v| v as i16)
}

/// Read a 0–3 byte little-endian unsigned integer.  Returns None if 0 bytes or out of bounds.
fn read_uint_le(p: &[u8], off: usize, width: usize) -> Option<u32> {
    if width == 0 || off + width > p.len() {
        return None;
    }
    let mut v = 0u32;
    for i in 0..width {
        v |= (p[off + i] as u32) << (8 * i);
    }
    Some(v)
}

/// Decode a type-6 PlayerBlock into player state.
///
/// Confirmed layout (from Structure6.xml + oracle experiments):
///   b8-9  : Homeworld planet index (LE uint16)
///   b16-81: Race design fields — IDENTICAL to .r1 layout (confirmed 2026-04-13, R0.9)
///   b26   : EnergyLevel       (IT=0, JOAT=3)
///   b27   : WeaponsLevel      (IT=0, JOAT=3)
///   b28   : PropulsionLevel   (IT=5, JOAT=3)
///   b29   : ConstructionLevel (IT=5, JOAT=3)
///   b30   : ElectronicsLevel  (IT=0, JOAT=3)
///   b31   : BiologyLevel      (IT=0, JOAT=3)
fn decode_type6(p: &[u8]) -> (Option<u16>, [Option<u8>; 6], Option<Race>) {
    let hw_idx = read_u16_le(p, 8);
    let techs = [
        if p.len() > 26 { Some(p[26]) } else { None }, // energy
        if p.len() > 27 { Some(p[27]) } else { None }, // weapons
        if p.len() > 28 { Some(p[28]) } else { None }, // propulsion
        if p.len() > 29 { Some(p[29]) } else { None }, // construction
        if p.len() > 30 { Some(p[30]) } else { None }, // electronics
        if p.len() > 31 { Some(p[31]) } else { None }, // biology
    ];
    let race = race_from_payload(p).ok();
    (hw_idx, techs, race)
}

fn decode_type13(p: &[u8]) -> Option<PlanetRecord> {
    if p.len() < 2 {
        return None;
    }
    let raw_word = u16::from_le_bytes([p[0], p[1]]);
    let planet_index = raw_word & 0x7FF;   // low 11 bits
    let owner_id = ((raw_word >> 11) & 0x1F) as u8;  // high 5 bits; 31=nobody
    let colonized = p.len() > 11;

    let base = PlanetRecord {
        planet_index,
        owner_id,
        colonized,
        conc_ironium:      if p.len() > 5  { Some(p[5])  } else { None },
        conc_boranium:     if p.len() > 6  { Some(p[6])  } else { None },
        conc_germanium:    if p.len() > 7  { Some(p[7])  } else { None },
        gravity:           if p.len() > 8  { Some(p[8])  } else { None },
        temperature:       if p.len() > 9  { Some(p[9])  } else { None },
        radiation:         if p.len() > 10 { Some(p[10]) } else { None },
        surface_ironium:  None,
        surface_boranium: None,
        surface_germanium: None,
        population:       None,
        mines:            None,
        factories:        None,
        defenses:         None,
    };
    if !colonized || p.len() < 14 {
        return Some(base);
    }

    // Variable-length colonised decode.
    // Assumes b4 = DepletionLength = 0 (non-depleted) and Terraformed flag = 0.
    // SurfaceLength byte b13 encodes four 2-bit widths (LSB-first):
    //   bits 0-1 = IroniumWidth, 2-3 = BoraniumWidth, 4-5 = GermaniumWidth, 6-7 = PopWidth
    let surf_len = p[13];
    let iron_w  = ((surf_len >> 0) & 3) as usize;
    let boran_w = ((surf_len >> 2) & 3) as usize;
    let germ_w  = ((surf_len >> 4) & 3) as usize;
    let pop_w   = ((surf_len >> 6) & 3) as usize;

    let mut off = 14usize;
    let surface_ironium   = read_uint_le(p, off, iron_w);  off += iron_w;
    let surface_boranium  = read_uint_le(p, off, boran_w); off += boran_w;
    let surface_germanium = read_uint_le(p, off, germ_w);  off += germ_w;
    let population        = read_uint_le(p, off, pop_w);   off += pop_w;

    // Installations block: present when PlanetInfo bit 11 (Installations) is set.
    let planet_info = u16::from_le_bytes([p[2], p[3]]);
    let has_inst = (planet_info >> 11) & 1 == 1;
    let mut mines = None;
    let mut factories = None;
    let mut defenses = None;
    if has_inst {
        off += 1; // ExcessPopulation (1 byte)
        // Installations: 3 bytes, Mines in bits 0-11, Factories in bits 12-23.
        if off + 3 <= p.len() {
            let raw = (p[off] as u32) | ((p[off+1] as u32) << 8) | ((p[off+2] as u32) << 16);
            mines     = Some((raw & 0xFFF) as u16);
            factories = Some(((raw >> 12) & 0xFFF) as u16);
            off += 3;
        }
        // Defenses: 2 bytes, low 12 bits = defenses count.
        if off + 2 <= p.len() {
            let raw = (p[off] as u16) | ((p[off+1] as u16) << 8);
            defenses = Some(raw & 0xFFF);
        }
    }

    Some(PlanetRecord {
        surface_ironium, surface_boranium, surface_germanium, population,
        mines, factories, defenses,
        ..base
    })
}

fn decode_type16(p: &[u8]) -> Option<FleetRecord> {
    if p.len() < 19 { return None; }
    let orbit = u16::from_le_bytes([p[6], p[7]]);
    // Trailing bytes encode battle plan and waypoint count.
    // 22-byte record: b19-20=0, b21=waypoint_count
    // 23-byte record: b19-20=0, b21=battle_plan_idx, b22=waypoint_count
    let (battle_plan_idx, waypoint_count) = match p.len() {
        22 => (None,         if p.len() > 21 { p[21] } else { 0 }),
        23 => (if p.len() > 21 { Some(p[21]) } else { None },
               if p.len() > 22 { p[22] } else { 0 }),
        _  => (None, 0),
    };
    Some(FleetRecord {
        fleet_index:     u16::from_le_bytes([p[0], p[1]]),
        orbit_planet_idx: orbit,
        en_route:        orbit == 65535,
        x:               i16::from_le_bytes([p[8], p[9]]),
        y:               i16::from_le_bytes([p[10], p[11]]),
        design_bitmask:  u16::from_le_bytes([p[12], p[13]]),
        num_stacks:      u16::from_le_bytes([p[14], p[15]]),
        ship_count:      p[16],
        total_mass_kt:   i16::from_le_bytes([p[17], p[18]]),
        battle_plan_idx,
        waypoint_count,
    })
}

/// Decode a single two-byte resource operation from the type-19 Transport payload.
///
/// Encoding (confirmed exp4):
///   lo = amount & 0xFF
///   hi = (action << 4) | ((amount >> 8) & 0xF)
fn decode_resource_op(lo: u8, hi: u8) -> ResourceOp {
    let amount = (lo as u16) | (((hi & 0x0F) as u16) << 8);
    let action = (hi >> 4) & 0x0F;
    ResourceOp { amount, action }
}

/// Decode a type-19 or type-20 waypoint payload.
/// `has_task_payload` is true for type-19 (b8-17 hold task parameters).
fn decode_waypoint(p: &[u8], has_task_payload: bool) -> Option<WaypointRecord> {
    if p.len() < 8 { return None; }
    let b6 = p[6];
    let b7 = p[7];
    let task_type = b6 & 0xF;

    // Decode Transport payload from b8-17 when present.
    let transport_payload = if has_task_payload && task_type == 1 && p.len() >= 18 {
        Some(TransportPayload {
            ironium:   decode_resource_op(p[8],  p[9]),
            boranium:  decode_resource_op(p[10], p[11]),
            germanium: decode_resource_op(p[12], p[13]),
            colonists: decode_resource_op(p[14], p[15]),
            fuel:      decode_resource_op(p[16], p[17]),
        })
    } else {
        None
    };

    Some(WaypointRecord {
        x:                   i16::from_le_bytes([p[0], p[1]]),
        y:                   i16::from_le_bytes([p[2], p[3]]),
        planet_index:        i16::from_le_bytes([p[4], p[5]]),
        warp_speed:          b6 >> 4,
        task_type,
        is_current_position: (b7 & 0x04) != 0,
        has_task_payload,
        transport_payload,
    })
}

fn decode_type30(p: &[u8]) -> Option<BattlePlanRecord> {
    if p.len() < 4 { return None; }
    Some(BattlePlanRecord {
        plan_id:          p[0],
        tactic:           p[1],
        primary_target:   p[2] & 0xF,
        secondary_target: (p[2] >> 4) & 0xF,
        attack_who:       p[3],
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

    let mut year: u32 = 2400;
    let mut homeworld_planet_idx: Option<u16> = None;
    let mut techs: [Option<u8>; 6] = [None; 6];
    let mut race: Option<Race> = None;
    let mut battle_plans: Vec<BattlePlanRecord> = Vec::new();
    let mut planets: Vec<PlanetRecord> = Vec::new();
    let mut fleets: Vec<FleetRecord> = Vec::new();
    let mut waypoints: Vec<WaypointRecord> = Vec::new();

    for rec in &records {
        match rec.rtype {
            8 => {
                if rec.payload.len() >= 12 {
                    let turn_raw = u16::from_le_bytes([rec.payload[10], rec.payload[11]]);
                    year = 2400 + turn_raw as u32;
                }
            }
            6 => {
                if homeworld_planet_idx.is_none() {
                    let (hw_idx, t, r) = decode_type6(&rec.payload);
                    homeworld_planet_idx = hw_idx;
                    techs = t;
                    race  = r;
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
            tech_energy:       techs[0],
            tech_weapons:      techs[1],
            tech_propulsion:   techs[2],
            tech_construction: techs[3],
            tech_electronics:  techs[4],
            tech_biology:      techs[5],
            race,
            battle_plans,
        },
        planets,
        fleets,
        waypoints,
    };

    let json = serde_json::to_string_pretty(&turn).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
