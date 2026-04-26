// Stars! record-container parser.
//
// All Stars! data files share the same container format:
//   - Sequence of 2-byte LE record headers followed by payloads
//   - Header: high 6 bits = record type, low 10 bits = payload length
//   - Type 8 (file header): plaintext 16-byte payload; seeds/re-seeds the LCG
//   - Type 0 (end marker): no payload
//   - All other types: XOR-encrypted payloads
//
// After the type-8 record is parsed, all subsequent records are decrypted
// using the LCG state derived from the type-8 payload.
//
// Type-8 payload layout (16 bytes; partially decoded):
//   bytes  0–9 : magic / fingerprint (constant across a session)
//   bytes 10–11: turn counter — LE u16, increments by 1 per host generation
//                  (game year = 2400 + turn_count).  Confirmed against .hst
//                  by running `stars.exe -gN` and observing the word advance
//                  by exactly N (R5.2, 2026-04-24).  Same offset works for
//                  .m1–.m16 player turn files.
//   bytes 12–13: LCG seed word (decrypts the rest of the file)
//   bytes 14–15: pre-advance derivation bytes

use crate::cipher::{decrypt, derive_pre_advance, derive_seeds, LcgState};

/// A parsed and decrypted record from a Stars! binary file.
#[derive(Debug)]
pub struct Record {
    pub rtype: u16,
    pub payload: Vec<u8>,
}

/// Parse a Stars! binary file, decrypt all payloads, and return the record list.
///
/// Returns an error string if the file header record is missing.
pub fn parse_file(bytes: &[u8]) -> Result<Vec<Record>, String> {
    let mut pos = 0usize;
    let mut lcg: Option<LcgState> = None;
    let mut records = Vec::new();

    while pos + 2 <= bytes.len() {
        let hdr_word = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        let rtype = hdr_word >> 10;
        let rlen  = (hdr_word & 0x3FF) as usize;
        pos += 2;

        let end = (pos + rlen).min(bytes.len());
        let raw = &bytes[pos..end];
        pos += rlen;

        match rtype {
            8 => {
                // Plaintext header; seeds or re-seeds the LCG.
                if raw.len() == 16 {
                    let seed_word = u16::from_le_bytes([raw[12], raw[13]]);
                    let (s1, s2) = derive_seeds(seed_word);
                    let mut state = LcgState::new(s1, s2);
                    state.advance(derive_pre_advance(raw));
                    lcg = Some(state);
                }
                records.push(Record { rtype, payload: raw.to_vec() });
            }
            0 => {
                // End-of-file marker; not decrypted.
                records.push(Record { rtype, payload: Vec::new() });
            }
            _ => {
                let mut payload = raw.to_vec();
                if let Some(ref mut state) = lcg {
                    decrypt(&mut payload, state);
                }
                records.push(Record { rtype, payload });
            }
        }
    }

    if lcg.is_none() {
        return Err("no type-8 file header record found".to_string());
    }
    Ok(records)
}

/// Return the decrypted payload of the first record with the given type,
/// or `None` if no such record exists.
pub fn first_payload_of_type(records: &[Record], rtype: u16) -> Option<&[u8]> {
    records.iter()
        .find(|r| r.rtype == rtype)
        .map(|r| r.payload.as_slice())
}

/// Read the turn counter from the type-8 file header.
///
/// The turn counter lives at payload offsets 10–11 (LE u16) and increments by
/// exactly 1 each time the host generates a turn.  Returns `None` if no type-8
/// record is present or its payload is shorter than 12 bytes.
pub fn turn_count(records: &[Record]) -> Option<u16> {
    let p = first_payload_of_type(records, 8)?;
    if p.len() < 12 { return None; }
    Some(u16::from_le_bytes([p[10], p[11]]))
}

/// Game year derived from the turn counter (2400 + turn_count).
pub fn game_year(records: &[Record]) -> Option<u32> {
    turn_count(records).map(|t| 2400 + t as u32)
}
