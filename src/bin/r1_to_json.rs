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

// ── Name encoding algorithm ───────────────────────────────────────────────────
// Confirmed 2026-04-22 via Ghidra decompilation of FUN_1070_551c,
// FUN_1040_45a0, and FUN_1040_4880 in stars.exe.
//
// All names (dropdown presets and user-typed) use the same nibble-packing
// algorithm.  Each character maps to a code; codes are nibble-packed
// high-nibble-first; odd nibble counts get a trailing 0xF pad nibble.
//
// Decoding: read nibble pairs from the key bytes to recover the original text.

fn decode_name_key(data: &[u8]) -> String {
    // Expand bytes to nibbles (high nibble first).
    let mut nib: Vec<u8> = Vec::with_capacity(data.len() * 2);
    for &b in data {
        nib.push(b >> 4);
        nib.push(b & 0xf);
    }

    // Map second nibble in the 'd' group (n2 4..=15) to lowercase letters.
    const D_GROUP: [char; 12] = ['b','c','d','f','g','j','k','m','p','q','u','v'];
    const E_GROUP: [char;  4] = ['w','x','y','z'];
    // Map code 0..=10 to characters (0=space, 1=a, 2=e, ..., 10=t).
    const ONE_NIB: [char; 11] = [' ','a','e','h','i','l','n','o','r','s','t'];

    let mut result = String::new();
    let mut i = 0usize;
    while i < nib.len() {
        let n1 = nib[i]; i += 1;
        match n1 {
            0..=10 => result.push(ONE_NIB[n1 as usize]),
            0xb => {
                let n2 = if i < nib.len() { nib[i] } else { 0xf }; i += 1;
                if n2 == 0xf { break; }
                result.push(char::from(0x41 + n2));    // A–P
            }
            0xc => {
                let n2 = if i < nib.len() { nib[i] } else { 0xf }; i += 1;
                if n2 == 0xf { break; }
                if n2 <= 9 { result.push(char::from(0x51 + n2)); }  // Q–Z
                else       { result.push(char::from(0x30 + n2 - 10)); } // 0–5
            }
            0xd => {
                let n2 = if i < nib.len() { nib[i] } else { 0xf }; i += 1;
                if n2 == 0xf { break; }
                if n2 <= 3 { result.push(char::from(0x36 + n2)); }  // 6–9
                else if (n2 as usize) - 4 < D_GROUP.len() {
                    result.push(D_GROUP[(n2 as usize) - 4]);
                }
            }
            0xe => {
                let n2 = if i < nib.len() { nib[i] } else { 0xf }; i += 1;
                if n2 == 0xf { break; }
                if (n2 as usize) < E_GROUP.len() { result.push(E_GROUP[n2 as usize]); }
            }
            0xf => {
                // 3-nibble sequence or trailing pad.
                if i + 1 < nib.len() {
                    let n2 = nib[i]; let n3 = nib[i + 1]; i += 2;
                    if n3 == 0xf { break; }
                    if let Some(c) = char::from_u32(((n3 as u32) << 4) | (n2 as u32)) {
                        result.push(c);
                    }
                } else { break; }
            }
            _ => {}
        }
    }
    result
}

// ── Name section decoder ─────────────────────────────────────────────────────
// Layout at payload[112..]:
//   [0]        : 0x00 constant
//   [1]        : singular key length
//   [2..2+n]   : singular key bytes (nibble-packed encoded name)
//   [2+n]      : plural key length (0 = absent; importer defaults to singular+'s')
//   …

fn decode_name_block(payload: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= payload.len() { return None; }
    let key_len = payload[start] as usize;
    if key_len == 0 { return None; }
    let data_end = (start + 1 + key_len).min(payload.len());
    let data = &payload[start + 1..data_end];
    Some((decode_name_key(data), start + 1 + key_len))
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
