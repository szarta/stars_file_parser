// Race data types and decoder — Rust representation of the Stars! race JSON schema.
//
// These types serialise to the race.json format defined in
// stars-reborn-design/docs/new_game/race_file_format.rst.
//
// The canonical engine copy lives in stars-reborn-engine; this copy is kept
// in sync as a standalone dependency for import tooling.
//
// race_from_payload() decodes the race design fields that live at bytes 16-81
// of the type-6 PlayerBlock.  These bytes have IDENTICAL layout in both .r1
// race files and .m player turn files (confirmed 2026-04-13 by cross-checking
// humanoid/JOAT known values against .m1 type-6 decrypted output).  Name,
// plural_name, and icon_index are NOT decoded here because their encoding
// differs between the two file types; callers fill those in afterwards when
// needed.

use serde::{Deserialize, Serialize};

// ── Primary Racial Trait ──────────────────────────────────────────────────────

/// .r1 byte value at offset 76: HE=0, SS=1, WM=2, CA=3, IS=4, SD=5, PP=6,
/// IT=7, AR=8, JOAT=9.  CA and AR are inferred (not yet oracle-confirmed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prt {
    #[serde(rename = "HE")]   He,
    #[serde(rename = "SS")]   Ss,
    #[serde(rename = "WM")]   Wm,
    #[serde(rename = "CA")]   Ca,
    #[serde(rename = "IS")]   Is,
    #[serde(rename = "SD")]   Sd,
    #[serde(rename = "PP")]   Pp,
    #[serde(rename = "IT")]   It,
    #[serde(rename = "AR")]   Ar,
    #[serde(rename = "JOAT")] Joat,
}

impl Prt {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::He),  1 => Some(Self::Ss),  2 => Some(Self::Wm),
            3 => Some(Self::Ca),  4 => Some(Self::Is),  5 => Some(Self::Sd),
            6 => Some(Self::Pp),  7 => Some(Self::It),  8 => Some(Self::Ar),
            9 => Some(Self::Joat),
            _ => None,
        }
    }
}

// ── Lesser Racial Traits ──────────────────────────────────────────────────────

/// 16-bit LE word at payload bytes 78-79.  Each LRT occupies one bit.
/// Confirmed by single-LRT differential experiments (2026-04-11, R1.2b).
///
/// File bit → LRT:
///   0→IFE  1→TT   2→ARM  3→ISB  4→GR   5→UR   6→MA   7→NRE
///   8→CE   9→OBRM 10→NAS 11→LSP 12→BET 13→RS
/// Bits 14-15 unused.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lrt {
    NRE,  // No Ramscoop Engines            — file bit 7
    IFE,  // Improved Fuel Efficiency       — file bit 0
    CE,   // Cheap Engines                  — file bit 8
    TT,   // Total Terraforming             — file bit 1
    OBRM, // Only Basic Remote Mining       — file bit 9
    ARM,  // Advanced Remote Mining         — file bit 2
    NAS,  // No Advanced Scanners           — file bit 10
    ISB,  // Improved Starbases             — file bit 3
    LSP,  // Low Starting Population        — file bit 11
    GR,   // Generalized Research           — file bit 4
    BET,  // Bleeding Edge Technology       — file bit 12
    UR,   // Ultimate Recycling             — file bit 5
    RS,   // Regenerating Shields           — file bit 13
    MA,   // Mineral Alchemy                — file bit 6
}

// ── Leftover spend ────────────────────────────────────────────────────────────

/// How the race designer spent leftover advantage points.
///
/// Stored at payload byte 69.  Confirmed 2026-04-15 by creating one race per
/// option in Stars! and reading the byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeftoverSpend {
    #[serde(rename = "surface_minerals")]      SurfaceMinerals,
    #[serde(rename = "mineral_concentrations")] MineralConcentrations,
    #[serde(rename = "mines")]                 Mines,
    #[serde(rename = "factories")]             Factories,
    #[serde(rename = "defenses")]              Defenses,
}

impl LeftoverSpend {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::SurfaceMinerals),
            1 => Some(Self::MineralConcentrations),
            2 => Some(Self::Mines),
            3 => Some(Self::Factories),
            4 => Some(Self::Defenses),
            _ => None,
        }
    }

    pub fn to_byte(&self) -> u8 {
        match self {
            Self::SurfaceMinerals      => 0,
            Self::MineralConcentrations => 1,
            Self::Mines                => 2,
            Self::Factories            => 3,
            Self::Defenses             => 4,
        }
    }
}

// ── Research cost ─────────────────────────────────────────────────────────────

/// Research cost multiplier for one tech area.
///
/// Byte values at payload offsets 70-75 (E/W/P/C/El/Bio):
///   0 → 75% extra cost  ("Expensive" in AIs.md, "75% extra" in Stars! UI)
///   1 → Normal/standard cost
///   2 → 50% extra cost  ("Cheap" in AIs.md, "50% extra" in Stars! UI)
///
/// The AIs.md community doc (Wumpus 2005/2007) labels byte-2 as "Cheap" and
/// byte-0 as "Expensive" — both are penalties above Normal.  "Cheap" in that
/// context means "the cheaper of the two expensive options" (50% vs 75%).
/// Confirmed 2026-04-14 via viewai inspection of AI .m files.
///
/// A genuine Cheap tier (research costs less than Normal) likely exists for
/// human-designed races but has not been observed in AI template data; its
/// byte value is unconfirmed (possibly 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TechCost {
    /// 75% extra research cost per level (byte value 0).
    /// Labeled "Expensive" in AIs.md community docs.
    #[serde(rename = "expensive_75")]
    Expensive75,
    /// Standard research cost (byte value 1).
    #[serde(rename = "normal")]
    Normal,
    /// 50% extra research cost per level (byte value 2).
    /// Labeled "Cheap" in AIs.md community docs (cheaper of two expensive tiers).
    #[serde(rename = "expensive_50")]
    Expensive50,
}

impl TechCost {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Expensive75),
            1 => Some(Self::Normal),
            2 => Some(Self::Expensive50),
            _ => None,
        }
    }
}

// ── Habitat ───────────────────────────────────────────────────────────────────

/// One habitat axis.  `immune: true` means the race is immune to this axis;
/// `min`/`max` are absent when immune.
/// Units: gravity in g (from Gravity_Map), temperature in °C, radiation in mR/yr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabAxis {
    pub immune: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl HabAxis {
    pub fn immune() -> Self { Self { immune: true, min: None, max: None } }
    pub fn range(min: f64, max: f64) -> Self { Self { immune: false, min: Some(min), max: Some(max) } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabPreferences {
    pub gravity: HabAxis,
    pub temperature: HabAxis,
    pub radiation: HabAxis,
}

// ── Economy ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Economy {
    pub resource_production: u32,
    pub factory_production: u32,
    pub factory_cost: u32,
    pub factory_cheap_germanium: bool,
    pub colonists_operate_factories: u32,
    pub mine_production: u32,
    pub mine_cost: u32,
    pub colonists_operate_mines: u32,
    pub growth_rate: u32,
}

// ── Research costs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCosts {
    pub energy: TechCost,
    pub weapons: TechCost,
    pub propulsion: TechCost,
    pub construction: TechCost,
    pub electronics: TechCost,
    pub biotechnology: TechCost,
    /// byte 81 bit 5: 'All "Costs 75% extra" research fields start at Tech 3'.
    /// Confirmed 2026-04-14 via viewai inspection.
    pub expensive_tech_start_at_3: bool,
}

// ── Race ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Race {
    pub format_version: u32,
    pub name: String,
    pub plural_name: String,
    pub prt: Prt,
    pub lrts: Vec<Lrt>,
    pub hab: HabPreferences,
    pub economy: Economy,
    pub research_costs: ResearchCosts,
    pub leftover_spend: LeftoverSpend,
    pub icon_index: u32,
}

// ── Habitat conversion tables ─────────────────────────────────────────────────

/// Gravity_Map: index 0–100 → g-value × 100 (centi-g).
/// Source: stars-reborn-design/docs/mechanics/habitability.rst.
/// Unreachable indices (1, 3, 5, 7, 10) use the nearest reachable value.
#[rustfmt::skip]
const GRAV_CENTI: [u16; 101] = [
    12, 12, 13, 13, 14, 14, 15, 15, 16, 17,  //  0-9
    17, 18, 19, 20, 21, 22, 24, 25, 27, 29,  // 10-19
    31, 33, 36, 40, 44, 50, 51, 52, 53, 54,  // 20-29
    55, 56, 58, 59, 60, 62, 64, 65, 67, 69,  // 30-39
    71, 73, 75, 78, 80, 83, 86, 89, 92, 96,  // 40-49
   100,104,108,112,116,120,124,128,132,136,  // 50-59
   140,144,148,152,156,160,164,168,172,176,  // 60-69
   180,184,188,192,196,200,224,248,272,296,  // 70-79
   320,344,368,392,416,440,464,488,512,536,  // 80-89
   560,584,608,632,656,680,704,728,752,776,  // 90-99
   800,                                       // 100
];

fn grav_from_index(idx: u8) -> f64 {
    GRAV_CENTI[idx as usize] as f64 / 100.0
}

/// Temperature: file index 0–100 → °C.  Formula: (idx − 50) × 4.
fn temp_from_index(idx: u8) -> f64 {
    (idx as i32 - 50) as f64 * 4.0
}

fn decode_hab_axis(center: u8, min_idx: u8, max_idx: u8, axis: u8) -> HabAxis {
    if center == 0xFF {
        return HabAxis::immune();
    }
    let (min_f, max_f) = match axis {
        0 => (grav_from_index(min_idx), grav_from_index(max_idx)),
        1 => (temp_from_index(min_idx), temp_from_index(max_idx)),
        _ => (min_idx as f64, max_idx as f64), // radiation: direct 0-100
    };
    HabAxis::range(min_f, max_f)
}

// ── LRT decoder ───────────────────────────────────────────────────────────────

/// Decode the 16-bit LRT bitmask from bytes 78-79.
///
/// File bit → LRT mapping confirmed by single-LRT differential experiments
/// (2026-04-11, R1.2b).  Same mapping applies to both .r1 and .m type-6 payloads.
fn decode_lrts(b78: u8, b79: u8) -> Vec<Lrt> {
    let word = (b78 as u16) | ((b79 as u16) << 8);
    const BITS: &[(u8, Lrt)] = &[
        (0,  Lrt::IFE),
        (1,  Lrt::TT),
        (2,  Lrt::ARM),
        (3,  Lrt::ISB),
        (4,  Lrt::GR),
        (5,  Lrt::UR),
        (6,  Lrt::MA),
        (7,  Lrt::NRE),
        (8,  Lrt::CE),
        (9,  Lrt::OBRM),
        (10, Lrt::NAS),
        (11, Lrt::LSP),
        (12, Lrt::BET),
        (13, Lrt::RS),
    ];
    BITS.iter()
        .filter(|(bit, _)| word & (1 << bit) != 0)
        .map(|(_, lrt)| lrt.clone())
        .collect()
}

// ── Shared payload decoder ────────────────────────────────────────────────────

/// Decode race design fields from a type-6 payload (bytes 16-81).
///
/// Works for both `.r1` race files and `.m` player turn files: bytes 16-81
/// have identical layout in both (confirmed 2026-04-13, R0.9).
///
/// `name`, `plural_name`, and `icon_index` are left as defaults (empty /
/// zero) because their encoding differs between file types.  Callers that
/// need those fields — specifically `r1_to_json` — fill them in afterwards.
pub fn race_from_payload(p: &[u8]) -> Result<Race, String> {
    if p.len() < 82 {
        return Err(format!("type-6 payload too short: {} bytes (expected ≥82)", p.len()));
    }

    let tech = |b: u8| -> Result<TechCost, String> {
        TechCost::from_byte(b).ok_or_else(|| format!("unknown tech-cost byte {b}"))
    };

    Ok(Race {
        format_version: 1,
        name:         String::new(),
        plural_name:  String::new(),
        icon_index:   0,
        prt: Prt::from_byte(p[76])
            .ok_or_else(|| format!("unknown PRT byte {}", p[76]))?,
        lrts: decode_lrts(p[78], p[79]),
        hab: HabPreferences {
            gravity:     decode_hab_axis(p[16], p[19], p[22], 0),
            temperature: decode_hab_axis(p[17], p[20], p[23], 1),
            radiation:   decode_hab_axis(p[18], p[21], p[24], 2),
        },
        economy: Economy {
            resource_production:         p[62] as u32 * 100,
            factory_production:          p[63] as u32,
            factory_cost:                p[64] as u32,
            factory_cheap_germanium:     (p[81] & 0x80) != 0,
            colonists_operate_factories: p[65] as u32,
            mine_production:             p[66] as u32,
            mine_cost:                   p[67] as u32,
            colonists_operate_mines:     p[68] as u32,
            growth_rate:                 p[25] as u32,
        },
        research_costs: ResearchCosts {
            energy:        tech(p[70])?,
            weapons:       tech(p[71])?,
            propulsion:    tech(p[72])?,
            construction:  tech(p[73])?,
            electronics:   tech(p[74])?,
            biotechnology: tech(p[75])?,
            expensive_tech_start_at_3: (p[81] & 0x20) != 0,
        },
        leftover_spend: LeftoverSpend::from_byte(p[69])
            .ok_or_else(|| format!("unknown leftover_spend byte {}", p[69]))?,
    })
}
