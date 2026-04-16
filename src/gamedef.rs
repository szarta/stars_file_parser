// Stars! game definition (.def) format.
//
// A `.def` file drives `stars.exe -a game.def` to create a new game without
// going through the GUI dialogs.  The format is a Windows text file (CRLF
// line endings) with the following structure:
//
//   Line 1:  Game name (free text)
//   Line 2:  <map_size> <density> <player_positions> <seed>
//              map_size:        0=Tiny 1=Small 2=Medium 3=Large 4=Huge
//              density:         1=Sparse 2=Normal 3=Dense 4=Packed
//              player_positions:0=Close 1=Moderate 2=Farther 3=Distant
//              seed:            32-bit integer; universe PRNG seed
//   Line 3:  7 checkbox flags (0/1): MaxMinerals SlowTech BBSPlay
//              GalaxyClumping ComputerAlliances NoRandomEvents PublicScores
//   Line 4:  NumPlayers (1–16)
//   Lines N: One line per player in order:
//              Human player: Windows path to the .r1 race file
//              AI player:    "# <difficulty> <param>"
//                              difficulty 0=easy 1=standard 2=harder 3=expert
//                              param: always 1 in observed files (purpose unknown)
//   Then, one victory-condition line per VC (disabled = bare "0"):
//     planets:        "1 <percent>"          or "0"
//     tech:           "1 <level> <fields>"   or "0"
//     score:          "1 <score>"            or "0"
//     exceeds_nearest:"1 <percent>"          or "0"
//     production:     "1 <capacity>"         or "0"
//     capital_ships:  "1 <number>"           or "0"
//     turns:          "1 <years>"            or "0"
//   Last VC line (always two values): "<must_meet> <min_years>"
//   Final line: Windows path for the output .xy file.
//
// Confirmed 2026-04-15 against web_captures/starsfaq/game.def and oracle test
// (original/cli_test/test2.def → produced TestGame.hst/.m1/.m2/.xy).
//
// Output files land in the CWD where stars.exe runs; game is named by
// <GameName>.hst, <GameName>.m1 … <GameName>.mN, <GameName>.xy.

use serde::{Deserialize, Serialize};

use crate::universe::MapSize;

// ── Density ──────────────────────────────────────────────────────────────────

/// Planet density of the generated universe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Sparse,
    Normal,
    Dense,
    Packed,
}

impl Density {
    pub fn to_index(&self) -> u8 {
        match self {
            Self::Sparse => 1,
            Self::Normal => 2,
            Self::Dense  => 3,
            Self::Packed => 4,
        }
    }

    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            1 => Some(Self::Sparse),
            2 => Some(Self::Normal),
            3 => Some(Self::Dense),
            4 => Some(Self::Packed),
            _ => None,
        }
    }
}

// ── PlayerPositions ───────────────────────────────────────────────────────────

/// Starting distance between homeworlds.
///
/// Exact integer values confirmed for Close(0) and Distant(3) from the Stars!
/// UI option list; intermediate values are inferred from order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerPositions {
    Close,
    Moderate,
    Farther,
    Distant,
}

impl PlayerPositions {
    pub fn to_index(&self) -> u8 {
        match self {
            Self::Close    => 0,
            Self::Moderate => 1,
            Self::Farther  => 2,
            Self::Distant  => 3,
        }
    }

    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Self::Close),
            1 => Some(Self::Moderate),
            2 => Some(Self::Farther),
            3 => Some(Self::Distant),
            _ => None,
        }
    }
}

// ── UniverseSetup ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseSetup {
    pub map_size: MapSize,
    pub density: Density,
    pub player_positions: PlayerPositions,
    /// Universe PRNG seed.  Determines planet layout, hab values, etc.
    /// Use a fixed value for reproducible oracle tests.
    pub seed: u32,
}

// ── GameOptions ───────────────────────────────────────────────────────────────

/// The seven boolean game-setup checkboxes in line 3 of the .def file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOptions {
    /// Begin all planets with maximum mineral concentrations.
    pub max_minerals: bool,
    /// All tech advances cost 75% extra (Slow Tech).
    pub slow_tech: bool,
    /// BBS/PBEM play mode (auto-advance without all players submitting).
    pub bbs_play: bool,
    /// Galaxy clumping — planets cluster rather than distribute uniformly.
    pub galaxy_clumping: bool,
    /// Computer players may form alliances with each other.
    pub computer_alliances: bool,
    /// No random events (mystery traders, comets, etc.).
    pub no_random_events: bool,
    /// Public scores visible to all players from turn 1.
    pub public_scores: bool,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self {
            max_minerals:       false,
            slow_tech:          false,
            bbs_play:           false,
            galaxy_clumping:    false,
            computer_alliances: false,
            no_random_events:   false,
            public_scores:      false,
        }
    }
}

// ── Player ────────────────────────────────────────────────────────────────────

/// One player slot in the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Player {
    /// Human player identified by a Windows path to their `.r1` race file.
    Human {
        /// Windows path to the `.r1` race file (e.g. `Z:\path\race.r1`).
        race_file: String,
    },
    /// Computer-controlled player.
    Ai {
        /// Difficulty tier: 0=easy 1=standard 2=harder 3=expert.
        difficulty: u8,
        /// Second parameter; always 1 in observed oracle files.  Purpose TBD.
        param: u8,
    },
}

// ── Victory conditions ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcPlanets {
    pub enabled: bool,
    /// Percentage of total planets that must be owned (20–100).
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcTech {
    pub enabled: bool,
    /// Minimum tech level required across `fields` tech areas (8–26).
    pub level: u8,
    /// Number of tech fields that must reach `level` (2–6).
    pub fields: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcScore {
    pub enabled: bool,
    /// Absolute score threshold (1000–20000).
    pub score: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcExceedsNearest {
    pub enabled: bool,
    /// Score must exceed the nearest rival's score by this percentage (20–300).
    pub percent: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcProduction {
    pub enabled: bool,
    /// Annual resource production threshold in thousands (10–500).
    pub capacity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcCapitalShips {
    pub enabled: bool,
    /// Number of capital ships required (10–300).
    pub number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcTurns {
    pub enabled: bool,
    /// Number of years that must elapse (30–900).
    pub years: u32,
}

/// All victory conditions for the game.
///
/// At least one VC should be enabled.  `must_meet` specifies how many of the
/// enabled conditions a player must simultaneously satisfy to win.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryConditions {
    pub planets:         VcPlanets,
    pub tech:            VcTech,
    pub score:           VcScore,
    pub exceeds_nearest: VcExceedsNearest,
    pub production:      VcProduction,
    pub capital_ships:   VcCapitalShips,
    pub turns:           VcTurns,
    /// Number of enabled VCs a player must satisfy simultaneously to win (0–7).
    /// Use 1 to require only one condition; use the total count to require all.
    pub must_meet: u8,
    /// Minimum number of years the game must run before a winner can be declared
    /// (30–500).
    pub min_years: u32,
}

// ── GameDef ───────────────────────────────────────────────────────────────────

/// A complete Stars! game definition, suitable for serialising to a `.def` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDef {
    pub game_name: String,
    pub universe:  UniverseSetup,
    pub options:   GameOptions,
    /// Players in order; the number of elements is the NumPlayers value.
    pub players:   Vec<Player>,
    pub victory:   VictoryConditions,
    /// Windows path where the game should write the output `.xy` file.
    pub output_xy: String,
}

// ── Serialiser ────────────────────────────────────────────────────────────────

fn map_size_index(s: &MapSize) -> u8 {
    match s {
        MapSize::Tiny   => 0,
        MapSize::Small  => 1,
        MapSize::Medium => 2,
        MapSize::Large  => 3,
        MapSize::Huge   => 4,
    }
}

fn flag(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// Serialise a `GameDef` to the bytes of a `.def` file with Windows CRLF
/// line endings.
///
/// The caller is responsible for writing the output to a file or stdout.
/// Stars! requires CRLF; feeding LF-only output to `stars.exe -a` will cause
/// silent failure (no output files created).
pub fn to_def_bytes(def: &GameDef) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();

    // Line 1 — game name
    lines.push(def.game_name.clone());

    // Line 2 — universe parameters
    lines.push(format!(
        "{} {} {} {}",
        map_size_index(&def.universe.map_size),
        def.universe.density.to_index(),
        def.universe.player_positions.to_index(),
        def.universe.seed,
    ));

    // Line 3 — option checkboxes
    let o = &def.options;
    lines.push(format!(
        "{} {} {} {} {} {} {}",
        flag(o.max_minerals),
        flag(o.slow_tech),
        flag(o.bbs_play),
        flag(o.galaxy_clumping),
        flag(o.computer_alliances),
        flag(o.no_random_events),
        flag(o.public_scores),
    ));

    // Line 4 — player count
    lines.push(def.players.len().to_string());

    // Player lines
    for player in &def.players {
        match player {
            Player::Human { race_file } => lines.push(race_file.clone()),
            Player::Ai { difficulty, param } => {
                lines.push(format!("# {} {}", difficulty, param));
            }
        }
    }

    // Victory condition lines
    let v = &def.victory;

    if v.planets.enabled {
        lines.push(format!("1 {}", v.planets.percent));
    } else {
        lines.push("0".to_string());
    }

    if v.tech.enabled {
        lines.push(format!("1 {} {}", v.tech.level, v.tech.fields));
    } else {
        lines.push("0".to_string());
    }

    if v.score.enabled {
        lines.push(format!("1 {}", v.score.score));
    } else {
        lines.push("0".to_string());
    }

    if v.exceeds_nearest.enabled {
        lines.push(format!("1 {}", v.exceeds_nearest.percent));
    } else {
        lines.push("0".to_string());
    }

    if v.production.enabled {
        lines.push(format!("1 {}", v.production.capacity));
    } else {
        lines.push("0".to_string());
    }

    if v.capital_ships.enabled {
        lines.push(format!("1 {}", v.capital_ships.number));
    } else {
        lines.push("0".to_string());
    }

    if v.turns.enabled {
        lines.push(format!("1 {}", v.turns.years));
    } else {
        lines.push("0".to_string());
    }

    // must_meet line — always two values
    lines.push(format!("{} {}", v.must_meet, v.min_years));

    // Output .xy path
    lines.push(def.output_xy.clone());

    // Join with CRLF and add a final CRLF
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    out.into_bytes()
}
