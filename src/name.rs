// Race-name decoder for type-6 PlayerBlock payloads.
//
// Confirmed 2026-04-22 via Ghidra decompilation of FUN_1070_551c,
// FUN_1040_45a0, and FUN_1040_4880 in stars.exe.
//
// All names — dropdown presets, 24 AI race names, and user-typed — use the same
// nibble-packing algorithm.  The layout is identical between `.r1` race files
// and `.m1`–`.m16` player turn files.
//
// Layout at payload[112..]:
//   [112]      : R = length of the player-relations array that follows (bytes)
//   [113..113+R]: relations array — one byte per player (the player's own slot
//                 is 0x00).  R is 0 when absent: standalone `.r1` race files and
//                 any block without game context.  In a game turn file R equals
//                 the player count, so the name section shifts right by R.
//   [113+R]    : singular key length
//   [113+R+1..]: singular key bytes (nibble-packed encoded name)
//   […+n]      : plural key length (0 = absent; default to singular + 's')
//   […+n+1..]  : plural key bytes
//
// NOTE: R was previously documented as a "0x00 constant" because the player-1
// `.m1` block and `.r1` files all have R=0; that garbled every other player's
// block, where R > 0 (confirmed across try2, ai_games, normally_played_game).
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
    const D_GROUP: [char; 12] = ['b', 'c', 'd', 'f', 'g', 'j', 'k', 'm', 'p', 'q', 'u', 'v'];
    // E_GROUP: codes 0..=3 are direct w/x/y/z.  Codes 4..=15 index a
    // special-character table (decompiled label FUN_1118_05c8) — the game's
    // own encoder uses this table to keep common punctuation in 2 nibbles
    // instead of falling through to the 3-nibble extended-char path.
    const E_GROUP: [char; 4] = ['w', 'x', 'y', 'z'];
    const E_TABLE: &[(u8, char)] = &[(0xc, '\'')];
    // Map code 0..=10 to characters (0=space, 1=a, 2=e, ..., 10=t).
    const ONE_NIB: [char; 11] = [' ', 'a', 'e', 'h', 'i', 'l', 'n', 'o', 'r', 's', 't'];

    let mut result = String::new();
    let mut i = 0usize;
    while i < nib.len() {
        let n1 = nib[i];
        i += 1;
        match n1 {
            0..=10 => result.push(ONE_NIB[n1 as usize]),
            0xb => {
                if i >= nib.len() {
                    break;
                }
                let n2 = nib[i];
                i += 1;
                result.push(char::from(0x41 + n2)); // A–P
            }
            0xc => {
                if i >= nib.len() {
                    break;
                }
                let n2 = nib[i];
                i += 1;
                if n2 <= 9 {
                    result.push(char::from(0x51 + n2));
                }
                // Q–Z
                else {
                    result.push(char::from(0x30 + n2 - 10));
                } // 0–5
            }
            0xd => {
                if i >= nib.len() {
                    break;
                }
                let n2 = nib[i];
                i += 1;
                if n2 <= 3 {
                    result.push(char::from(0x36 + n2));
                }
                // 6–9
                else if (n2 as usize) - 4 < D_GROUP.len() {
                    result.push(D_GROUP[(n2 as usize) - 4]);
                }
            }
            0xe => {
                if i >= nib.len() {
                    break;
                }
                let n2 = nib[i];
                i += 1;
                if (n2 as usize) < E_GROUP.len() {
                    result.push(E_GROUP[n2 as usize]);
                } else if let Some(&(_, c)) = E_TABLE.iter().find(|(code, _)| *code == n2) {
                    result.push(c);
                }
            }
            0xf => {
                // 3-nibble sequence or trailing pad.
                if i + 1 < nib.len() {
                    let n2 = nib[i];
                    let n3 = nib[i + 1];
                    i += 2;
                    if n3 == 0xf {
                        break;
                    }
                    if let Some(c) = char::from_u32(((n3 as u32) << 4) | (n2 as u32)) {
                        result.push(c);
                    }
                } else {
                    break;
                }
            }
            _ => {}
        }
    }
    result
}

/// Decode one length-prefixed name block at `start`, returning (name, next_start).
fn decode_name_block(payload: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= payload.len() {
        return None;
    }
    let key_len = payload[start] as usize;
    if key_len == 0 {
        return None;
    }
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

    // payload[112] is the length (in bytes) of an intervening player-relations
    // array — one byte per player (the player's own slot is 0x00), appended
    // before the name section.  It is 0 only when the array is absent, in which
    // case the name begins immediately at 113.  The player-1 .m1 block happens
    // to have a 0-length array, which is why hard-coding base+1 decoded that
    // block but garbled every other player's block (confirmed across try2,
    // ai_games, normally_played_game: the byte equals the game's player count).
    let relations_len = payload[base] as usize;
    let name_start = base + 1 + relations_len;
    if name_start >= payload.len() {
        return (String::new(), String::new());
    }

    let (singular, plural_start) = match decode_name_block(payload, name_start) {
        Some(pair) => pair,
        None => return (String::new(), String::new()),
    };

    let plural = decode_name_block(payload, plural_start)
        .map(|(n, _)| n)
        .unwrap_or_else(|| format!("{singular}s"));

    (singular, plural)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_nibble_letters() {
        // Codes 1,1,1 ('a','a','a') packed high-nibble-first with a trailing 0xf
        // pad nibble.
        assert_eq!(decode_name_key(&[0x11, 0x1f]), "aaa");
    }

    #[test]
    fn plural_defaults_to_singular_plus_s_when_absent() {
        let mut payload = vec![0u8; 117];
        payload[112] = 0x00; // relations array length 0 (name starts at 113)
        payload[113] = 0x02; // singular key length
        payload[114] = 0x11; // "aaa"
        payload[115] = 0x1f;
        payload[116] = 0x00; // plural length 0 → block absent

        let (singular, plural) = decode_names(&payload);
        assert_eq!(singular, "aaa");
        assert_eq!(plural, "aaas");
    }

    #[test]
    fn skips_nonzero_relations_array_before_the_name() {
        // A game turn file for a non-player-1 block: payload[112] is the length
        // of the player-relations array (one byte per player, own slot 0x00),
        // and the name section begins only after it.  Here: 3 players, so the
        // name sits at 113 + 3 = 116, not 113.
        let mut payload = vec![0u8; 122];
        payload[112] = 0x03; // relations array length (3 players)
        payload[113] = 0x02; // player 0: neutral
        payload[114] = 0x00; // player 1: self
        payload[115] = 0x02; // player 2: neutral
        payload[116] = 0x02; // singular key length → "aaa"
        payload[117] = 0x11;
        payload[118] = 0x1f;
        payload[119] = 0x01; // plural key length → "aa"
        payload[120] = 0x11;

        let (singular, plural) = decode_names(&payload);
        assert_eq!(
            singular, "aaa",
            "name must be read after the relations array"
        );
        assert_eq!(plural, "aa");
    }

    #[test]
    fn decodes_an_explicit_plural_block() {
        let mut payload = vec![0u8; 118];
        payload[112] = 0x00;
        payload[113] = 0x02; // singular "aaa"
        payload[114] = 0x11;
        payload[115] = 0x1f;
        payload[116] = 0x01; // plural key length
        payload[117] = 0x11; // "aa" (codes 1,1 — no pad needed)

        let (singular, plural) = decode_names(&payload);
        assert_eq!(singular, "aaa");
        assert_eq!(
            plural, "aa",
            "explicit plural block must override the default"
        );
    }

    #[test]
    fn empty_payload_yields_empty_names() {
        assert_eq!(decode_names(&[0u8; 8]), (String::new(), String::new()));
    }
}
