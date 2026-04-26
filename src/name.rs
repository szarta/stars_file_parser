// Race-name decoder for type-6 PlayerBlock payloads.
//
// Confirmed 2026-04-22 via Ghidra decompilation of FUN_1070_551c,
// FUN_1040_45a0, and FUN_1040_4880 in stars.exe.
//
// All names — dropdown presets, 24 AI race names, and user-typed — use the same
// nibble-packing algorithm at payload[112..].  The layout is identical between
// `.r1` race files and `.m1`–`.m16` player turn files (verified 2026-04-25 via
// 16-player collision-resolution oracle in
// `stars-reborn-research/original/all_human_players_{rabbitoid,humanoid}/`).
//
// Layout at payload[112..]:
//   [112]      : 0x00 constant
//   [113]      : singular key length
//   [114..]    : singular key bytes (nibble-packed encoded name)
//   [114+n]    : plural key length (0 = absent; default to singular + 's')
//   [115+n..]  : plural key bytes
//
// Encoding: each character maps to a code; codes are nibble-packed
// high-nibble-first; odd nibble counts get a trailing 0xF pad nibble.

/// Decode a nibble-packed Stars! race-name key into its UTF-8 string.
pub fn decode_name_key(data: &[u8]) -> String {
    let mut nib: Vec<u8> = Vec::with_capacity(data.len() * 2);
    for &b in data {
        nib.push(b >> 4);
        nib.push(b & 0xf);
    }

    // Map second nibble in the 'd' group (n2 4..=15) to lowercase letters.
    const D_GROUP: [char; 12] = ['b','c','d','f','g','j','k','m','p','q','u','v'];
    // E_GROUP: codes 0..=3 are direct w/x/y/z.  Codes 4..=15 index a
    // special-character table (decompiled label FUN_1118_05c8) — the game's
    // own encoder uses this table to keep common punctuation in 2 nibbles
    // instead of falling through to the 3-nibble extended-char path.
    const E_GROUP: [char;  4] = ['w','x','y','z'];
    const E_TABLE: &[(u8, char)] = &[
        (0xc, '\''),
    ];
    // Map code 0..=10 to characters (0=space, 1=a, 2=e, ..., 10=t).
    const ONE_NIB: [char; 11] = [' ','a','e','h','i','l','n','o','r','s','t'];

    let mut result = String::new();
    let mut i = 0usize;
    while i < nib.len() {
        let n1 = nib[i]; i += 1;
        match n1 {
            0..=10 => result.push(ONE_NIB[n1 as usize]),
            0xb => {
                if i >= nib.len() { break; }
                let n2 = nib[i]; i += 1;
                result.push(char::from(0x41 + n2));    // A–P
            }
            0xc => {
                if i >= nib.len() { break; }
                let n2 = nib[i]; i += 1;
                if n2 <= 9 { result.push(char::from(0x51 + n2)); }  // Q–Z
                else       { result.push(char::from(0x30 + n2 - 10)); } // 0–5
            }
            0xd => {
                if i >= nib.len() { break; }
                let n2 = nib[i]; i += 1;
                if n2 <= 3 { result.push(char::from(0x36 + n2)); }  // 6–9
                else if (n2 as usize) - 4 < D_GROUP.len() {
                    result.push(D_GROUP[(n2 as usize) - 4]);
                }
            }
            0xe => {
                if i >= nib.len() { break; }
                let n2 = nib[i]; i += 1;
                if (n2 as usize) < E_GROUP.len() {
                    result.push(E_GROUP[n2 as usize]);
                } else if let Some(&(_, c)) = E_TABLE.iter().find(|(code, _)| *code == n2) {
                    result.push(c);
                }
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

/// Decode one length-prefixed name block at `start`, returning (name, next_start).
fn decode_name_block(payload: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= payload.len() { return None; }
    let key_len = payload[start] as usize;
    if key_len == 0 { return None; }
    let data_end = (start + 1 + key_len).min(payload.len());
    let data = &payload[start + 1..data_end];
    Some((decode_name_key(data), start + 1 + key_len))
}

/// Decode the singular and plural race names from a type-6 payload.
///
/// Returns `(singular, plural)`.  If the singular block is missing, both are
/// empty strings.  If the plural block is missing, defaults to `singular + "s"`
/// (matches Stars! UI behavior for races where no plural was entered).
pub fn decode_names(payload: &[u8]) -> (String, String) {
    let base = 112;
    if payload.len() <= base + 1 {
        return (String::new(), String::new());
    }

    let (singular, plural_start) = match decode_name_block(payload, base + 1) {
        Some(pair) => pair,
        None       => return (String::new(), String::new()),
    };

    let plural = decode_name_block(payload, plural_start)
        .map(|(n, _)| n)
        .unwrap_or_else(|| format!("{singular}s"));

    (singular, plural)
}
