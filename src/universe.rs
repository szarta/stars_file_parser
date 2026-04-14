// Universe parameters decoded from the .xy file type-7 record.
//
// The .xy file (universe coordinate/map file) contains a type-7 record at
// payload bytes 0-63 that encodes game setup parameters.  Confirmed fields
// (2026-04-14, from comparing all 4 difficulties × 5 map sizes × 13 runs):
//
//   word[0]  (b0-1)  : game seed — unique per game, varies across runs
//   word[2]  (b4-5)  : map size index: 0=Tiny, 1=Small, 2=Medium, 3=Large, 4=Huge
//   word[3]  (b6-7)  : always 1 (unknown)
//   word[4]  (b8-9)  : intended total player count (human + AI), before homeworld
//                      placement cuts; actual player count may be lower for
//                      harder/expert games where the universe cannot fit all
//                      requested homeworlds at the required spacing
//   word[5]  (b10-11): total planet count in the universe
//                      (32/128/288/512/800 for Tiny/Small/Medium/Large/Huge)
//   word[6]  (b12-13): difficulty A: 1 = easy|standard, 2 = harder|expert
//   word[7]  (b14-15): always 0
//   word[8]  (b16-17): difficulty B: 4 = easy|standard|harder, 20 = expert
//
// The combination of word[6] and word[8] identifies three distinguishable
// difficulty tiers.  Easy and Standard share an identical signature and cannot
// be separated from the .xy file alone.

use serde::{Deserialize, Serialize};

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
/// Easy and Standard share an identical binary signature; they can only be
/// distinguished by comparing the AI race templates (see R4.3 in PLAN.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    /// word[6]=1, word[8]=4 — Easy or Standard (indistinguishable from .xy alone)
    #[serde(rename = "easy|standard")]
    EasyOrStandard,
    /// word[6]=2, word[8]=4
    #[serde(rename = "harder")]
    Harder,
    /// word[6]=2, word[8]=20
    #[serde(rename = "expert")]
    Expert,
}

impl Difficulty {
    /// Decode from the two distinguishing word values in the type-7 payload.
    pub fn from_words(diff_a: u16, diff_b: u16) -> Option<Self> {
        match (diff_a, diff_b) {
            (1, 4)  => Some(Self::EasyOrStandard),
            (2, 4)  => Some(Self::Harder),
            (2, 20) => Some(Self::Expert),
            _       => None,
        }
    }
}

// ── UniverseParams ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseParams {
    pub format_version: u32,
    pub map_size: MapSize,
    pub planet_count: u32,
    /// Total player count the game was *set up* with (human + AI).
    /// For harder/expert games this may exceed the number of .m files actually
    /// generated, because the homeworld placement algorithm drops players it
    /// cannot fit at the required minimum spacing.
    pub intended_player_count: u32,
    pub difficulty: Difficulty,
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

    let size_idx  = w(2);
    let n_players = w(4);
    let planet_ct = w(5);
    let diff_a    = w(6);
    let diff_b    = w(8);

    let map_size = MapSize::from_index(size_idx)
        .ok_or_else(|| format!("unknown map size index {size_idx}"))?;

    let difficulty = Difficulty::from_words(diff_a, diff_b)
        .ok_or_else(|| format!(
            "unknown difficulty signature (word[6]={diff_a}, word[8]={diff_b})"
        ))?;

    Ok(UniverseParams {
        format_version: 1,
        map_size,
        planet_count: planet_ct as u32,
        intended_player_count: n_players as u32,
        difficulty,
    })
}
