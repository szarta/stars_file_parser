// Universe parameters decoded from the .xy file type-7 record.
//
// The .xy file (universe coordinate/map file) contains a type-7 record at
// payload bytes 0-63 that encodes game setup parameters.  Confirmed fields
// (2026-04-14 initial; 2026-04-22 density + galaxy_clumping added):
//
//   word[0]  (b0-1)  : game seed — unique per game, varies across runs
//   word[2]  (b4-5)  : map size index: 0=Tiny, 1=Small, 2=Medium, 3=Large, 4=Huge
//   word[3]  (b6-7)  : density: 1=Sparse, 2=Normal, 3=Dense (max; see note)
//                      NOTE: No word[3]=4 has ever been observed.  The Stars!
//                      UI's highest option ("Packed") stores as code 3, the same
//                      as Dense.  The game.def density=4 token is invalid; Stars!
//                      exits 0 with no output when given density=4 headlessly.
//   word[4]  (b8-9)  : intended total player count (human + AI), before homeworld
//                      placement cuts; actual player count may be lower for
//                      harder/expert games where the universe cannot fit all
//                      requested homeworlds at the required spacing
//   word[5]  (b10-11): total planet count in the universe
//   word[6]  (b12-13): difficulty A — partial decode; see Difficulty enum
//   word[7]  (b14-15): always 0
//   word[8]  (b16-17): difficulty B — bit 8 (0x100) set when Galaxy Clumping is on;
//                      low byte: 4 = most difficulty configs, 20 = one variant
//
// The combination of word[6] and (word[8] & 0xFF) identifies difficulty tiers.
// Easy and Standard share an identical signature and cannot be separated from
// the .xy file alone.  diff_a=0 and diff_a=3 are observed but not yet decoded.

use serde::{Deserialize, Serialize};

// ── Density ───────────────────────────────────────────────────────────────────

/// Planet density of the generated universe, as stored in the .xy type-7 record.
///
/// The highest observable density code is 3 (Dense).  The Stars! UI's "Packed"
/// option also stores as code 3; there is no observed code 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XyDensity {
    Sparse,
    Normal,
    Dense,
}

impl XyDensity {
    pub fn from_index(idx: u16) -> Option<Self> {
        match idx {
            1 => Some(Self::Sparse),
            2 => Some(Self::Normal),
            3 => Some(Self::Dense),
            _ => None,
        }
    }
}

// ── Map size ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
}

impl MapSize {
    pub fn from_index(idx: u16) -> Option<Self> {
        match idx {
            0 => Some(Self::Tiny),
            1 => Some(Self::Small),
            2 => Some(Self::Medium),
            3 => Some(Self::Large),
            4 => Some(Self::Huge),
            _ => None,
        }
    }
}

// ── Difficulty ────────────────────────────────────────────────────────────────

/// AI difficulty level as encoded in the .xy type-7 record.
///
/// Decoded from word[6] (diff_a) and (word[8] & 0xFF) (diff_b_low).
/// Galaxy Clumping sets bit 8 of word[8] independently of difficulty.
/// Easy and Standard share an identical binary signature and cannot be
/// distinguished from the .xy file alone.
/// diff_a=0 and diff_a=3 are observed but their Stars! UI labels are not yet
/// confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    /// word[6]=0 — observed but label unconfirmed
    Unknown0,
    /// word[6]=1, word[8]&0xFF=4 — Easy or Standard (indistinguishable)
    #[serde(rename = "easy|standard")]
    EasyOrStandard,
    /// word[6]=2, word[8]&0xFF=4
    Harder,
    /// word[6]=3 — observed but label unconfirmed
    Unknown3,
    /// word[6]=2, word[8]&0xFF=20
    Expert,
}

impl Difficulty {
    /// Decode from the two distinguishing word values in the type-7 payload.
    /// Galaxy Clumping (bit 8 of word[8]) is masked off before comparison.
    pub fn from_words(diff_a: u16, diff_b: u16) -> Option<Self> {
        let diff_b_low = diff_b & 0xFF;
        match (diff_a, diff_b_low) {
            (0, _)   => Some(Self::Unknown0),
            (1, 4)   => Some(Self::EasyOrStandard),
            (2, 4)   => Some(Self::Harder),
            (2, 20)  => Some(Self::Expert),
            (3, _)   => Some(Self::Unknown3),
            _        => None,
        }
    }
}

// ── UniverseParams ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseParams {
    pub format_version: u32,
    pub map_size: MapSize,
    pub density: Option<XyDensity>,
    pub planet_count: u32,
    /// Total player count the game was *set up* with (human + AI).
    /// For harder/expert games this may exceed the number of .m files actually
    /// generated, because the homeworld placement algorithm drops players it
    /// cannot fit at the required minimum spacing.
    pub intended_player_count: u32,
    pub difficulty: Option<Difficulty>,
    /// Galaxy Clumping option was enabled (bit 8 of word[8] in type-7 payload).
    pub galaxy_clumping: bool,
}

// ── Decoder ───────────────────────────────────────────────────────────────────

/// Decode universe parameters from a decrypted type-7 payload.
pub fn universe_from_type7_payload(p: &[u8]) -> Result<UniverseParams, String> {
    if p.len() < 18 {
        return Err(format!(
            "type-7 payload too short: {} bytes (expected ≥18)", p.len()
        ));
    }

    let w = |i: usize| u16::from_le_bytes([p[i * 2], p[i * 2 + 1]]);

    let size_idx    = w(2);
    let density_idx = w(3);
    let n_players   = w(4);
    let planet_ct   = w(5);
    let diff_a      = w(6);
    let diff_b      = w(8);

    let map_size = MapSize::from_index(size_idx)
        .ok_or_else(|| format!("unknown map size index {size_idx}"))?;

    let density        = XyDensity::from_index(density_idx);
    let difficulty     = Difficulty::from_words(diff_a, diff_b);
    let galaxy_clumping = (diff_b & 0x100) != 0;

    Ok(UniverseParams {
        format_version: 1,
        map_size,
        density,
        planet_count: planet_ct as u32,
        intended_player_count: n_players as u32,
        difficulty,
        galaxy_clumping,
    })
}
