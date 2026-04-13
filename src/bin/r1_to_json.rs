// r1_to_json — read a Stars! .r1 race file and emit race.json on stdout.
//
// Usage:  r1_to_json <file.r1>
//
// The output is a single JSON object conforming to the race schema defined in
// stars-reborn-design/docs/new_game/race_file_format.rst.
//
// Known limitations
// -----------------
// - Gravity values use the Gravity_Map lookup table from habitability.rst.
//   Indices 1, 3, 5, 7, and 10 are "unreachable" in the original game; they
//   are assigned the nearest reachable value.

use std::{env, path::Path, process};

use stars_file_parser::{
    race::race_from_payload,
    records::{first_payload_of_type, parse_file},
};

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

    let mut race = race_from_payload(payload).unwrap_or_else(|e| {
        eprintln!("decode error: {e}");
        process::exit(1);
    });

    // Fill in the .r1-specific fields that race_from_payload leaves as defaults.
    let (name, plural_name) = decode_names(payload);
    race.name        = name;
    race.plural_name = plural_name;
    // payload[6]: bits 3-7 = icon_1idx mod 32, bits 0-2 = 0b111 (constant).
    // icon_0idx = ((payload[6] >> 3) - 1) & 0x1F  (0-indexed, 0-31)
    race.icon_index  = (((payload[6] >> 3) as u32).wrapping_sub(1)) & 0x1F;

    let json = serde_json::to_string_pretty(&race).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
