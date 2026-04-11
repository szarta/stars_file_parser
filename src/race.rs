// Race data types — Rust representation of the Stars! race JSON schema.
//
// These types serialise to the race.json format defined in
// stars-reborn-design/docs/new_game/race_file_format.rst.
//
// The canonical engine copy lives in stars-reborn-engine; this copy is kept
// in sync as a standalone dependency for import tooling.

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

/// Byte offset and bit encoding within the .r1 payload are not yet confirmed
/// (bytes 78-79 vary but encoding is unknown).  See research task R1.2.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lrt {
    NRE,  // No Ramscoop Engines
    IFE,  // Improved Fuel Efficiency
    CE,   // Cheap Engines
    TT,   // Total Terraforming
    OBRM, // Only Basic Remote Mining
    ARM,  // Advanced Remote Mining
    NAS,  // No Advanced Scanners
    ISB,  // Improved Starbases
    LSP,  // Low Starting Population
    GR,   // Generalized Research
    BET,  // Bleeding Edge Technology
    UR,   // Ultimate Recycling
    RS,   // Regenerating Shields
    MA,   // Mineral Alchemy
}

// ── Research cost ─────────────────────────────────────────────────────────────

/// .r1 byte value at offsets 70-75: 0=Expensive, 1=Normal, 2=Cheap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TechCost {
    Expensive,
    Normal,
    Cheap,
}

impl TechCost {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Expensive),
            1 => Some(Self::Normal),
            2 => Some(Self::Cheap),
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
    pub icon_index: u32,
}
