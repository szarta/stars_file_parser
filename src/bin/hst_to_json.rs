// hst_to_json — read a Stars! .hst host file and emit planet data on stdout.
//
// Usage:  hst_to_json <file.hst>
//
// Output is a JSON object with:
//   "year"    — game year (turn + 2400) from type-8 file header
//   "planets" — list of ALL planets in the universe (inhabited and uninhabited),
//               one entry per type-13 PlanetBlock record
//
// The host file differs from player turn files (.mN) in that it contains
// type-13 records for every planet in the universe — not just those visible to
// one player.  This makes it the authoritative source for concentration data
// without any scanner noise, fleet routing, or StarMOD dependency.
//
// Confirmed methodology (R3.4, 2026-04-20): parsing the .hst directly gives
// exact concentrations for all ~919 planets of a large/dense game in seconds.
//
// Field layout for type-13 PlanetBlock (confirmed from oracle experiments):
//   b0-1: PlanetIDAndOwnerID word — low 11 bits = planet index, high 5 bits = owner
//           owner 31 = uninhabited; 0 = player 1, 1 = player 2, …
//   b2-3: PlanetInfo flags word
//   b4:   DepletionLength — three 2-bit fields encoding extra bytes before concentrations
//   b5+depl: concentrations and hab values (6 bytes: iron, boran, germ, grav, temp, rad)
//
// Note: b4 (DepletionLength) offsets the concentration bytes; confirmed 2026-04-17.

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::records::{game_year, parse_file};

// ── Output types ─────────────────────────────────────────────────────────────

/// Planet record decoded from a type-13 PlanetBlock.
///
/// All inhabited and uninhabited planets are present in the host file.
/// Surface minerals and installations are decoded for colonized planets.
#[derive(Debug, Serialize)]
struct Planet {
    /// 0-based planet index (low 11 bits of the PlanetIDAndOwnerID word).
    planet_index: u16,
    /// Owner ID (high 5 bits; 31 = uninhabited, 0 = player 1, …).
    owner_id: u8,
    /// Ironium   concentration (1–119 confirmed; 0 = uninhabited/depleted).
    #[serde(skip_serializing_if = "Option::is_none")]
    conc_ironium: Option<u8>,
    /// Boranium  concentration.
    #[serde(skip_serializing_if = "Option::is_none")]
    conc_boranium: Option<u8>,
    /// Germanium concentration.
    #[serde(skip_serializing_if = "Option::is_none")]
    conc_germanium: Option<u8>,
    /// Gravity index (0–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    gravity: Option<u8>,
    /// Temperature index (0–100; °C = (idx − 50) × 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<u8>,
    /// Radiation (0–100 mR/yr).
    #[serde(skip_serializing_if = "Option::is_none")]
    radiation: Option<u8>,
    /// Colony population in units of 100 colonists (colonized planets only).
    #[serde(skip_serializing_if = "Option::is_none")]
    population: Option<u32>,
    /// Surface ironium in kT (colonized planets only).
    #[serde(skip_serializing_if = "Option::is_none")]
    surface_ironium: Option<u32>,
    /// Surface boranium in kT (colonized planets only).
    #[serde(skip_serializing_if = "Option::is_none")]
    surface_boranium: Option<u32>,
    /// Surface germanium in kT (colonized planets only).
    #[serde(skip_serializing_if = "Option::is_none")]
    surface_germanium: Option<u32>,
}

/// Full host-file dump.
#[derive(Debug, Serialize)]
struct HostState {
    /// Game year (type-8 header: turn + 2400).
    year: u32,
    /// Count of planets decoded (convenience field for quick sanity checks).
    planet_count: usize,
    planets: Vec<Planet>,
}

// ── Decoder ───────────────────────────────────────────────────────────────────

fn read_u16_le(p: &[u8], off: usize) -> Option<u16> {
    if off + 2 <= p.len() {
        Some(u16::from_le_bytes([p[off], p[off + 1]]))
    } else {
        None
    }
}

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

fn decode_type13(p: &[u8]) -> Option<Planet> {
    if p.len() < 2 {
        return None;
    }
    let raw_word    = u16::from_le_bytes([p[0], p[1]]);
    let planet_index = raw_word & 0x7FF;
    let owner_id    = ((raw_word >> 11) & 0x1F) as u8;

    // b4 = DepletionLength: three 2-bit fields encoding byte-widths of depletion
    // accumulators inserted before the concentration bytes.
    let depl       = if p.len() > 4 { p[4] } else { 0 };
    let iron_dw    = ((depl >> 0) & 3) as usize;
    let boran_dw   = ((depl >> 2) & 3) as usize;
    let germ_dw    = ((depl >> 4) & 3) as usize;
    let depl_total = iron_dw + boran_dw + germ_dw;

    let cb        = 5 + depl_total;
    let colonized = p.len() > 11 + depl_total;

    let conc_ironium   = if p.len() > cb     { Some(p[cb])     } else { None };
    let conc_boranium  = if p.len() > cb + 1 { Some(p[cb + 1]) } else { None };
    let conc_germanium = if p.len() > cb + 2 { Some(p[cb + 2]) } else { None };
    let gravity        = if p.len() > cb + 3 { Some(p[cb + 3]) } else { None };
    let temperature    = if p.len() > cb + 4 { Some(p[cb + 4]) } else { None };
    let radiation      = if p.len() > cb + 5 { Some(p[cb + 5]) } else { None };

    if !colonized {
        return Some(Planet {
            planet_index, owner_id,
            conc_ironium, conc_boranium, conc_germanium,
            gravity, temperature, radiation,
            population: None,
            surface_ironium: None, surface_boranium: None, surface_germanium: None,
        });
    }

    let surf_len_off = 13 + depl_total;
    if p.len() <= surf_len_off {
        return Some(Planet {
            planet_index, owner_id,
            conc_ironium, conc_boranium, conc_germanium,
            gravity, temperature, radiation,
            population: None,
            surface_ironium: None, surface_boranium: None, surface_germanium: None,
        });
    }

    let surf_len = p[surf_len_off];
    let iron_w   = ((surf_len >> 0) & 3) as usize;
    let boran_w  = ((surf_len >> 2) & 3) as usize;
    let germ_w   = ((surf_len >> 4) & 3) as usize;
    let pop_w    = ((surf_len >> 6) & 3) as usize;

    let mut off = 14 + depl_total;
    let surface_ironium   = read_uint_le(p, off, iron_w);  off += iron_w;
    let surface_boranium  = read_uint_le(p, off, boran_w); off += boran_w;
    let surface_germanium = read_uint_le(p, off, germ_w);  off += germ_w;
    let population        = read_uint_le(p, off, pop_w);

    let _ = (read_u16_le, off); // suppress unused warning

    Some(Planet {
        planet_index, owner_id,
        conc_ironium, conc_boranium, conc_germanium,
        gravity, temperature, radiation,
        population, surface_ironium, surface_boranium, surface_germanium,
    })
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
    let mut planets: Vec<Planet> = Vec::new();

    for rec in &records {
        if rec.rtype == 13 {
            if let Some(p) = decode_type13(&rec.payload) {
                planets.push(p);
            }
        }
    }

    let planet_count = planets.len();
    let state = HostState { year, planet_count, planets };

    let json = serde_json::to_string_pretty(&state).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
