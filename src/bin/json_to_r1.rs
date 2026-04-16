// json_to_r1 — write a Stars! .r1 race file from a race JSON.
//
// Usage:
//   json_to_r1 <input.json> <output.r1>
//   json_to_r1 - <output.r1>            (read JSON from stdin)
//
// Input format: the JSON schema produced by r1_to_json (same Race struct).
//
// The output file uses a canonical fixed type-8 header.  b4-7 are zeroed;
// seed_word is fixed at 0x521F (the value used by all oracle-experiment test
// files, confirmed working in the original game).  This gives deterministic
// encryption: s1=61, s2=251, pre_advance=4.  Output bytes will differ from
// the original .r1 even for a perfect semantic round-trip — Stars! only
// cares about the decrypted payload.
//
// Known limitations
// -----------------
// - User-typed names shorter than 8 characters are padded with spaces.
// - The plural name is always written explicitly; if you round-trip a file
//   that relied on the r1_to_json fallback (singular + 's'), the output file
//   will contain explicit plural bytes, but the decoded name is unchanged.

use std::{env, io::Read, path::Path, process};

use stars_file_parser::{
    cipher::{decrypt, derive_pre_advance, derive_seeds, LcgState},
    race::{HabAxis, Lrt, Prt, Race, TechCost},
};

// ── Canonical type-8 header ───────────────────────────────────────────────────
// Layout confirmed from all known .r1 files:
//   b0-3:  'J3J3' magic (required)
//   b4-7:  session/game ID — varies per file; zeros are accepted (observed in
//           test.r1 used in oracle experiments)
//   b8-9:  0x2A60 — Stars! version constant; EVERY known .r1 uses this value;
//           the game rejects files where this field is wrong
//   b10-11: 0x0000 — turn offset (0 for race design files)
//   b12-13: seed_word = 0 → derive_seeds(0): s1=3, s2=139
//   b14-15: 0x0005 — dts flags constant; present in every .r1 file
//
// Cipher effect: seed_word=0x521F → derive_seeds → s1=61, s2=251;
// p1=0, p4=0, p5=31, p6=0 → pre_advance = 1*1*4+0 = 4.
// 0x521F was chosen because every oracle-experiment test.r1 file uses this
// seed_word with b4-7=0.  Any non-zero seed_word would work; the correct
// type-0 checksum (computed from plaintext XOR, not the cipher key) is what
// Stars! actually validates.
const CANONICAL_HEADER: [u8; 16] = [
    0x4A, 0x33, 0x4A, 0x33, // 'J3J3' magic
    0x00, 0x00, 0x00, 0x00, // b4-7:  session ID (zeros OK)
    0x60, 0x2A, 0x00, 0x00, // b8-9:  version 0x2A60; b10-11: turn 0
    0x1F, 0x52, 0x05, 0x00, // b12-13: seed_word=0x521F; b14-15: dts flags 0x0005
];

// ── Gravity reverse lookup ────────────────────────────────────────────────────
// Mirror of the GRAV_CENTI table in race.rs: index 0–100 → centi-g.
#[rustfmt::skip]
const GRAV_CENTI: [u16; 101] = [
    12, 12, 13, 13, 14, 14, 15, 15, 16, 17,   //   0-9
    17, 18, 19, 20, 21, 22, 24, 25, 27, 29,   //  10-19
    31, 33, 36, 40, 44, 50, 51, 52, 53, 54,   //  20-29
    55, 56, 58, 59, 60, 62, 64, 65, 67, 69,   //  30-39
    71, 73, 75, 78, 80, 83, 86, 89, 92, 96,   //  40-49
   100,104,108,112,116,120,124,128,132,136,   //  50-59
   140,144,148,152,156,160,164,168,172,176,   //  60-69
   180,184,188,192,196,200,224,248,272,296,   //  70-79
   320,344,368,392,416,440,464,488,512,536,   //  80-89
   560,584,608,632,656,680,704,728,752,776,   //  90-99
   800,                                        // 100
];

/// Convert g-value to GRAV_CENTI index by nearest-centi-g match.
fn grav_to_index(g: f64) -> u8 {
    let centi = (g * 100.0).round() as i32;
    GRAV_CENTI
        .iter()
        .enumerate()
        .min_by_key(|(_, &v)| (v as i32 - centi).unsigned_abs())
        .map(|(i, _)| i as u8)
        .unwrap_or(50)
}

/// Convert temperature °C to file index: idx = (temp / 4 + 50).round(), clamped 0–100.
fn temp_to_index(temp: f64) -> u8 {
    (temp / 4.0 + 50.0).round().clamp(0.0, 100.0) as u8
}

// ── Hab axis encoder ──────────────────────────────────────────────────────────
/// Returns (center_idx, min_idx, max_idx).  Immune axis → (0xFF, 0xFF, 0xFF).
fn encode_hab_axis(axis: &HabAxis, is_grav: bool, is_temp: bool) -> (u8, u8, u8) {
    if axis.immune {
        return (0xFF, 0xFF, 0xFF);
    }
    let min_f = axis.min.unwrap_or(0.0);
    let max_f = axis.max.unwrap_or(100.0);
    let (min_idx, max_idx) = if is_grav {
        (grav_to_index(min_f), grav_to_index(max_f))
    } else if is_temp {
        (temp_to_index(min_f), temp_to_index(max_f))
    } else {
        // Radiation: direct 0–100 index.
        (min_f.round().clamp(0.0, 100.0) as u8, max_f.round().clamp(0.0, 100.0) as u8)
    };
    let center_idx = ((min_idx as u16 + max_idx as u16) / 2) as u8;
    (center_idx, min_idx, max_idx)
}

// ── LRT encoder ───────────────────────────────────────────────────────────────
/// Build the 16-bit LE LRT bitmask from a slice of LRT values.
fn encode_lrts(lrts: &[Lrt]) -> (u8, u8) {
    let mut word: u16 = 0;
    for lrt in lrts {
        let bit: u8 = match lrt {
            Lrt::IFE  =>  0, Lrt::TT   =>  1, Lrt::ARM  =>  2, Lrt::ISB  =>  3,
            Lrt::GR   =>  4, Lrt::UR   =>  5, Lrt::MA   =>  6, Lrt::NRE  =>  7,
            Lrt::CE   =>  8, Lrt::OBRM =>  9, Lrt::NAS  => 10, Lrt::LSP  => 11,
            Lrt::BET  => 12, Lrt::RS   => 13,
        };
        word |= 1 << bit;
    }
    ((word & 0xFF) as u8, (word >> 8) as u8)
}

// ── PRT encoder ───────────────────────────────────────────────────────────────
fn encode_prt(prt: &Prt) -> u8 {
    match prt {
        Prt::He   => 0, Prt::Ss   => 1, Prt::Wm  => 2, Prt::Ca  => 3,
        Prt::Is   => 4, Prt::Sd   => 5, Prt::Pp  => 6, Prt::It  => 7,
        Prt::Ar   => 8, Prt::Joat => 9,
    }
}

// ── TechCost encoder ──────────────────────────────────────────────────────────
fn encode_tech_cost(tc: &TechCost) -> u8 {
    match tc {
        TechCost::Expensive => 0,
        TechCost::Normal    => 1,
        TechCost::Cheap     => 2,
    }
}

// ── Preset name encoding ──────────────────────────────────────────────────────
// Returns the preset byte sequence for the six default races (singular and
// plural).  For any other name, returns None and the caller falls through to
// user-typed encoding.
fn preset_bytes(name: &str) -> Option<Vec<u8>> {
    match name {
        "Humanoid"    => Some(vec![183, 222, 219, 22, 116, 214]),
        "Humanoids"   => Some(vec![183, 222, 219, 22, 116, 214, 159]),
        "Antetheral"  => Some(vec![176, 106,  42, 50, 129,  95]),
        "Antheherals" => Some(vec![176, 106,  42, 50, 129,  89]),
        "Insectoid"   => Some(vec![184, 105,  45, 90, 116, 214]),
        "Insectoids"  => Some(vec![184, 105,  45, 90, 116, 214, 159]),
        "Nucleotid"   => Some(vec![189, 222, 213, 82, 122,  77, 111]),
        "Nucleotids"  => Some(vec![189, 222, 213, 82, 122,  77, 105]),
        "Rabbitoid"   => Some(vec![193,  29,  77, 68, 167,  77, 111]),
        "Rabbitoids"  => Some(vec![193,  29,  77, 68, 167,  77, 105]),
        "Silicanoid"  => Some(vec![194,  69,  77, 81, 103,  77, 111]),
        "Silicanoids" => Some(vec![194,  69,  77, 81, 103,  77, 105]),
        _             => None,
    }
}

/// Encode a name to raw bytes suitable for inclusion in the name section.
///
/// For preset names: returns the opaque preset byte sequence.
/// For user-typed names: each character is encoded as (ascii + 111).
/// User-typed names shorter than 8 characters are padded with spaces
/// (space = 32, encoded as 32 + 111 = 143) to keep the marker ≥ 8.
/// This is needed because the r1_to_json decoder treats marker ≤ 7 as a
/// preset block; markers ≥ 8 are user-typed.
fn encode_name_bytes(name: &str) -> Vec<u8> {
    if let Some(bytes) = preset_bytes(name) {
        return bytes;
    }
    let mut bytes: Vec<u8> = name.bytes().map(|b| b.wrapping_add(111)).collect();
    while bytes.len() < 8 {
        bytes.push(32u8.wrapping_add(111)); // pad with space
    }
    bytes
}

/// Build the name section starting at payload offset 112.
///
/// Layout:
///   [0]          : 0x00 constant
///   [1]          : singular marker (= len of singular data bytes)
///   [2..2+marker]: singular data bytes
///   [2+marker]   : plural marker (0 = absent; present otherwise)
///   …
///
/// The plural is always written explicitly unless plural_name is empty.
fn encode_name_section(singular: &str, plural: &str) -> Vec<u8> {
    let mut section = vec![0u8]; // constant 0 at payload[112]

    let s_bytes = encode_name_bytes(singular);
    section.push(s_bytes.len() as u8);
    section.extend_from_slice(&s_bytes);

    if plural.is_empty() {
        section.push(0u8); // no plural; r1_to_json fallback will append 's'
    } else {
        let p_bytes = encode_name_bytes(plural);
        section.push(p_bytes.len() as u8);
        section.extend_from_slice(&p_bytes);
    }

    section
}

// ── Build plaintext type-6 payload ────────────────────────────────────────────
fn build_payload(race: &Race) -> Vec<u8> {
    let name_section = encode_name_section(&race.name, &race.plural_name);
    let mut p = vec![0u8; 112 + name_section.len()];

    // b0: constant 0xFF for .r1 race files (confirmed across all known files).
    p[0] = 0xFF;

    // b6: icon byte.  Encoding: bits 3–7 = icon_1idx (0-indexed + 1, mod 32);
    //     bits 0–2 = 0b111 (constant).
    //     icon_1idx = (icon_0idx + 1) & 0x1F
    let icon_1idx = (race.icon_index.wrapping_add(1) & 0x1F) as u8;
    p[6] = (icon_1idx << 3) | 0x07;

    // b16–18: hab axis centers (average of min and max indices).
    // b19–21: min indices. b22–24: max indices.
    let (gc, gmin, gmax) = encode_hab_axis(&race.hab.gravity,     true,  false);
    let (tc, tmin, tmax) = encode_hab_axis(&race.hab.temperature, false, true);
    let (rc, rmin, rmax) = encode_hab_axis(&race.hab.radiation,   false, false);
    p[16] = gc;   p[17] = tc;   p[18] = rc;
    p[19] = gmin; p[20] = tmin; p[21] = rmin;
    p[22] = gmax; p[23] = tmax; p[24] = rmax;

    // b25: growth rate.
    p[25] = race.economy.growth_rate as u8;

    // b56: constant 15 (0x0F) — observed in every .r1 file; purpose unknown.
    p[56] = 15;

    // b62–68: economy fields.
    p[62] = (race.economy.resource_production / 100) as u8;
    p[63] = race.economy.factory_production         as u8;
    p[64] = race.economy.factory_cost               as u8;
    p[65] = race.economy.colonists_operate_factories as u8;
    p[66] = race.economy.mine_production            as u8;
    p[67] = race.economy.mine_cost                  as u8;
    p[68] = race.economy.colonists_operate_mines    as u8;
    p[69] = race.leftover_spend.to_byte();

    // b70–75: research costs.
    p[70] = encode_tech_cost(&race.research_costs.energy);
    p[71] = encode_tech_cost(&race.research_costs.weapons);
    p[72] = encode_tech_cost(&race.research_costs.propulsion);
    p[73] = encode_tech_cost(&race.research_costs.construction);
    p[74] = encode_tech_cost(&race.research_costs.electronics);
    p[75] = encode_tech_cost(&race.research_costs.biotechnology);

    // b76: PRT.
    p[76] = encode_prt(&race.prt);

    // b78–79: LRT bitmask (16-bit LE).
    let (lrt_lo, lrt_hi) = encode_lrts(&race.lrts);
    p[78] = lrt_lo;
    p[79] = lrt_hi;

    // b81: flags. bit 7 = factory_cheap_germanium, bit 5 = expensive_tech_start_at_3.
    let mut flags = 0u8;
    if race.economy.factory_cheap_germanium           { flags |= 0x80; }
    if race.research_costs.expensive_tech_start_at_3  { flags |= 0x20; }
    p[81] = flags;

    // b112+: name section.
    p[112..].copy_from_slice(&name_section);

    p
}

// ── Type-0 checksum ───────────────────────────────────────────────────────────
// Stars! stores a 2-byte checksum as the type-0 end-of-file record payload.
// Formula (confirmed against all 29 known .r1 files, 2026-04-15):
//   idx = payload_len % 4
//   t0[0] = XOR(plaintext[even indices]) XOR C0[idx]
//   t0[1] = XOR(plaintext[odd indices])  XOR C1[idx]
//
// Constants (indexed by payload_len % 4):
const T0_C0: [u8; 4] = [0x72, 0x75, 0x3a, 0xaf];
const T0_C1: [u8; 4] = [0x9f, 0x00, 0x07, 0xaf];

fn compute_type0(plain: &[u8]) -> [u8; 2] {
    let (mut xe, mut xo) = (0u8, 0u8);
    for (i, &b) in plain.iter().enumerate() {
        if i % 2 == 0 { xe ^= b; } else { xo ^= b; }
    }
    let idx = plain.len() % 4;
    [xe ^ T0_C0[idx], xo ^ T0_C1[idx]]
}

// ── File serialiser ───────────────────────────────────────────────────────────
fn write_record(out: &mut Vec<u8>, rtype: u16, payload: &[u8]) {
    let hdr_word = (rtype << 10) | (payload.len() as u16);
    out.extend_from_slice(&hdr_word.to_le_bytes());
    out.extend_from_slice(payload);
}

fn build_file(race: &Race) -> Vec<u8> {
    let mut out = Vec::new();

    // Type-8: plaintext canonical header.
    write_record(&mut out, 8, &CANONICAL_HEADER);

    // Initialise cipher from canonical header — matches parse_file() on read.
    let seed_word = u16::from_le_bytes([CANONICAL_HEADER[12], CANONICAL_HEADER[13]]);
    let (s1, s2) = derive_seeds(seed_word);
    let mut lcg = LcgState::new(s1, s2);
    lcg.advance(derive_pre_advance(&CANONICAL_HEADER));

    // Type-6: encrypt (XOR is self-inverse) the race payload.
    let plain_payload = build_payload(race);
    let t0 = compute_type0(&plain_payload);
    let mut payload = plain_payload;
    decrypt(&mut payload, &mut lcg); // XOR == encrypt here
    write_record(&mut out, 6, &payload);

    // Type-0: end-of-file marker with checksum payload.
    write_record(&mut out, 0, &t0);

    out
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: json_to_r1 <input.json | -> <output.r1>");
        process::exit(1);
    }

    let json_str = if args[1] == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap_or_else(|e| {
            eprintln!("error reading stdin: {e}");
            process::exit(1);
        });
        s
    } else {
        std::fs::read_to_string(&args[1]).unwrap_or_else(|e| {
            eprintln!("error reading {}: {e}", args[1]);
            process::exit(1);
        })
    };

    let race: Race = serde_json::from_str(&json_str).unwrap_or_else(|e| {
        eprintln!("JSON parse error: {e}");
        process::exit(1);
    });

    let file_bytes = build_file(&race);

    let out_path = Path::new(&args[2]);
    std::fs::write(out_path, &file_bytes).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", out_path.display());
        process::exit(1);
    });
}
