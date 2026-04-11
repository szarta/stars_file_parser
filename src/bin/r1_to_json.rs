// r1_to_json — read a Stars! .r1 race file and emit race.json on stdout.
//
// Usage:  r1_to_json <file.r1>
//
// The output is a single JSON object conforming to the race schema defined in
// stars-reborn-design/docs/new_game/race_file_format.rst.
//
// Known limitations
// -----------------
// - `lrts` is always `[]`.  Bytes 78-79 of the payload encode LRTs and the
//   icon index, but their exact bit layout is not yet confirmed (R1.2).
// - `icon_index` is always 0 for the same reason.
// - Gravity values use the Gravity_Map lookup table from habitability.rst.
//   Indices 1, 3, 5, 7, and 10 are "unreachable" in the original game; they
//   are assigned the nearest reachable value.

use std::{env, path::Path, process};

use stars_file_parser::{
    race::{Economy, HabAxis, HabPreferences, Prt, Race, ResearchCosts, TechCost},
    records::{first_payload_of_type, parse_file},
};

// ── Habitat conversion tables ────────────────────────────────────────────────

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

// ── Preset name lookup ────────────────────────────────────────────────────────
// Maps the full data bytes of a preset name block to the name string.
// Singular and plural blocks share the same first bytes but differ in the last
// byte (and sometimes the marker).  Both are stored explicitly here.

fn lookup_preset_name(data: &[u8]) -> Option<&'static str> {
    match data {
        &[183, 222, 219, 22, 116, 214]      => Some("Humanoid"),
        &[183, 222, 219, 22, 116, 214, 159] => Some("Humanoids"),
        &[176, 106, 42, 50, 129, 95]        => Some("Antetheral"),
        &[176, 106, 42, 50, 129, 89]        => Some("Antheherals"),
        &[184, 105, 45, 90, 116, 214]       => Some("Insectoid"),
        &[184, 105, 45, 90, 116, 214, 159]  => Some("Insectoids"),
        &[189, 222, 213, 82, 122, 77, 111]  => Some("Nucleotid"),
        &[189, 222, 213, 82, 122, 77, 105]  => Some("Nucleotids"),
        &[193, 29, 77, 68, 167, 77, 111]    => Some("Rabbitoid"),
        &[193, 29, 77, 68, 167, 77, 105]    => Some("Rabbitoids"),
        &[194, 69, 77, 81, 103, 77, 111]    => Some("Silicanoid"),
        &[194, 69, 77, 81, 103, 77, 105]    => Some("Silicanoids"),
        _                                   => None,
    }
}

// ── Name section decoder ─────────────────────────────────────────────────────
// Layout at payload[112..]:
//   [0]          : 0x00 constant
//   [1]          : singular block marker  (6 or 7 = preset; 8+ = user-typed)
//   [2..2+marker]: marker bytes of data
//   [2+marker]   : plural block marker (or 0 = absent)
//   …
//
// Block marker = number of data bytes that follow.
// Preset blocks: data is an opaque lookup key.
// User-typed blocks: each byte b encodes char = (b − 111).

fn decode_name_block(payload: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= payload.len() { return None; }
    let marker = payload[start] as usize;
    if marker == 0 { return None; }
    let data_end = (start + 1 + marker).min(payload.len());
    let data = &payload[start + 1..data_end];

    let name = if marker <= 7 {
        lookup_preset_name(data)
            .map(|s| s.to_owned())
            .unwrap_or_else(|| {
                let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
                format!("<preset:{hex}>")
            })
    } else {
        data.iter().map(|&b| (b.wrapping_sub(111)) as char).collect()
    };

    Some((name, start + 1 + marker))
}

fn decode_names(payload: &[u8]) -> (String, String) {
    let base = 112;
    if payload.len() <= base + 1 { return (String::new(), String::new()); }

    let (singular, plural_start) = match decode_name_block(payload, base + 1) {
        Some(pair) => pair,
        None       => return (String::new(), String::new()),
    };

    let plural = decode_name_block(payload, plural_start)
        .map(|(n, _)| n)
        .unwrap_or_else(|| format!("{singular}s"));

    (singular, plural)
}

// ── Race decoder ─────────────────────────────────────────────────────────────

fn race_from_payload(p: &[u8]) -> Result<Race, String> {
    if p.len() < 82 {
        return Err(format!("type-6 payload too short: {} bytes (expected ≥82)", p.len()));
    }

    let tech = |b: u8| -> Result<TechCost, String> {
        TechCost::from_byte(b).ok_or_else(|| format!("unknown tech-cost byte {b}"))
    };

    let (name, plural_name) = decode_names(p);

    Ok(Race {
        format_version: 1,
        name,
        plural_name,
        prt: Prt::from_byte(p[76])
            .ok_or_else(|| format!("unknown PRT byte {}", p[76]))?,
        lrts: vec![], // bytes 78-79 encoding unconfirmed; see R1.2
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
        },
        icon_index: 0, // bytes 78-79 encoding unconfirmed; see R1.2
    })
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: r1_to_json <file.r1>");
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

    let payload = first_payload_of_type(&records, 6).unwrap_or_else(|| {
        eprintln!("no type-6 race record found in {}", path.display());
        process::exit(1);
    });

    let race = race_from_payload(payload).unwrap_or_else(|e| {
        eprintln!("decode error: {e}");
        process::exit(1);
    });

    let json = serde_json::to_string_pretty(&race).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
