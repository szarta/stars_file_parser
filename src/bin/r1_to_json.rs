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
    name::decode_names,
    race::race_from_payload,
    records::{first_payload_of_type, parse_file},
};

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

#[cfg(test)]
mod tests {
    use stars_file_parser::name::decode_name_key;

    /// Encoder mirroring `_char_code` + `encode_name_key` in
    /// `stars-reborn-research/reverse-engineering/scripts/analyze_r1.py`.
    /// Used only here to round-trip the decoder.
    fn encode(name: &str) -> Vec<u8> {
        fn char_code(c: char) -> u32 {
            let o = c as u32;
            if o == 0x20 { return 0x00; }
            if (0x61..=0x7a).contains(&o) {
                // lowercase letters
                let lower = [
                    ('a', 0x01u32), ('b', 0x4d), ('c', 0x5d), ('d', 0x6d), ('e', 0x02),
                    ('f', 0x7d),    ('g', 0x8d), ('h', 0x03), ('i', 0x04), ('j', 0x9d),
                    ('k', 0xad),    ('l', 0x05), ('m', 0xbd), ('n', 0x06), ('o', 0x07),
                    ('p', 0xcd),    ('q', 0xdd), ('r', 0x08), ('s', 0x09), ('t', 0x0a),
                    ('u', 0xed),    ('v', 0xfd), ('w', 0x0e), ('x', 0x1e), ('y', 0x2e),
                    ('z', 0x3e),
                ];
                return lower.iter().find(|(ch, _)| *ch == c).map(|&(_, v)| v).unwrap();
            }
            if (0x41..=0x50).contains(&o) { return ((o - 0x41) << 4) | 0x0b; }
            if (0x51..=0x5a).contains(&o) { return ((o - 0x51) << 4) | 0x0c; }
            if (0x30..=0x35).contains(&o) { return ((o - 0x26) << 4) | 0x0c; }
            if (0x36..=0x39).contains(&o) { return ((o - 0x36) << 4) | 0x0d; }
            (o << 4) | 0x0f
        }
        let mut nib: Vec<u8> = Vec::new();
        for ch in name.chars() {
            let mut code = char_code(ch);
            let n = if code < 0xb { 1 } else if (code & 0xf) == 0xf { 3 } else { 2 };
            for _ in 0..n {
                nib.push((code & 0xf) as u8);
                code >>= 4;
            }
        }
        if nib.len() % 2 == 1 { nib.push(0xf); }
        nib.chunks(2).map(|c| (c[0] << 4) | c[1]).collect()
    }

    fn round_trip(name: &str) {
        let bs = encode(name);
        let back = decode_name_key(&bs);
        assert_eq!(back, name, "round-trip failed: encoded={bs:02x?}");
    }

    #[test]
    fn round_trip_24_ai_race_names_and_dropdowns() {
        let names = [
            // dropdown presets
            "Humanoid", "Antetheral", "Insectoid", "Nucleotid", "Rabbitoid", "Silicanoid",
            // 24 AI race names
            "American", "Berserker", "Bulushi", "Cleaver", "Crusher", "Eagle", "Felite",
            "Ferret", "Golem", "Hawk", "Hicardi", "Hooveron", "House Cat", "Kurkonian",
            "Loraxoid", "Mensoid", "Nairnian", "Nee", "Nulon", "Picardi", "Rush'n",
            "Tritizoid", "Ubert", "Valadiac",
        ];
        for n in names { round_trip(n); }
    }

    /// Regression coverage for the bug fixed alongside this test:
    /// names containing 'P', '5', or 'v' encode their N2 nibble as 0xf and
    /// previously caused the decoder to treat 0xf as the trailing pad.
    #[test]
    fn round_trip_n2_eq_pad_chars() {
        round_trip("P");
        round_trip("5");
        round_trip("v");
        round_trip("Pv");
        round_trip("Pickv");
        round_trip("5v");
    }
}
