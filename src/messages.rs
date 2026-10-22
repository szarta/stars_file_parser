// EventsBlock (record type-12) decoding — partial.
//
// EventsBlock holds the player's per-turn message log: each event references a
// message-template ID (0..386) plus argument bytes that fill in the template's
// `\p`, `\s`, `\i`, etc. format codes.  The complete template table comes from
// Wasp's 2010 HWF post and is shipped as ``data/messages.json`` (loaded at
// build time via ``include_str!``).
//
// What's confirmed about per-event byte layout:
//   * First u16 LE = message identifier.  Low 10 bits = msg ID; high 6 bits
//     are non-zero in some samples (purpose unknown — possibly an event
//     category or addressee tag).  Use ``mid & 0x3FF`` to look up the format
//     string.
//   * Next u16 LE = "object" reference (planet idx, fleet ref, race idx) when
//     the message is location-bound, or one of the sentinels ``0xFFFF``,
//     ``0xFFFE``, ``0xFFFC`` when it isn't.  Confirmed values:
//       0xFFFF — tip messages (no location)
//       0xFFFE — research / non-located events
//       0xFFFC — winner declarations (msgs 181/182)
//   * After that come N argument bytes whose width-per-arg depends on the
//     individual format codes — not yet fully reverse-engineered.  Confirmed
//     widths so far on small maps (planet idx < 256):
//       \p (planet)     = 1 byte (planet idx)
//       \Z (race name)  = 1 byte (race idx)
//       \j (tech field) = 1 byte
//       \i (integer)    = 1 or 2 bytes (context-dependent)
//   * On large maps (>256 planets) the planet ref appears to widen to 2 bytes
//     with a leading "type" byte (sample: msg 169 encoded as
//     ``a9 02 94 01 94 01``).  Not yet specified.
//
// Because of those uncertainties, this module deliberately surfaces:
//   1. The message ID and its format string (always reliable),
//   2. The full raw payload bytes (for downstream R&D),
//   3. The first 4-byte header decoded as (msg_id, loc/sentinel),
// rather than attempting to walk the full event list.  R5.2 winner-message
// detection only needs (msg_id, race_idx) which is cleanly extractable from a
// 5-byte event.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Wasp's message-format-string table (387 entries, IDs 0..386).
///
/// Each entry records the number of `\X` substitution arguments the template
/// expects (column ARGS in the source post) and the template string itself.
#[derive(Debug, Clone, Serialize)]
pub struct MessageTemplate {
    pub args: u8,
    pub fmt: String,
}

const RAW_TABLE: &str = include_str!("../data/messages.json");

/// Lazy-loaded global lookup table (msg_id → MessageTemplate).
pub fn template_table() -> &'static HashMap<u16, MessageTemplate> {
    static TABLE: OnceLock<HashMap<u16, MessageTemplate>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let parsed: HashMap<String, serde_json::Value> =
            serde_json::from_str(RAW_TABLE).expect("messages.json must parse");
        let mut out = HashMap::with_capacity(parsed.len());
        for (k, v) in parsed {
            let id: u16 = k.parse().expect("messages.json keys must be u16");
            let args = v["args"].as_u64().unwrap_or(0) as u8;
            let fmt = v["fmt"].as_str().unwrap_or("").to_string();
            out.insert(id, MessageTemplate { args, fmt });
        }
        out
    })
}

/// Look up a message template by its ID.  Strips the high-6-bits tag from
/// the raw u16 so callers can pass the unmasked word.
pub fn lookup(raw_mid: u16) -> Option<&'static MessageTemplate> {
    template_table().get(&(raw_mid & 0x3FF))
}

/// One EventsBlock (type-12) record, partially decoded.
///
/// `header_msg_id` is the masked low-10-bit message ID at byte 0; on real
/// records this is reliably the first event's template ID.  `header_loc` is
/// the u16 LE word at offset 2 (planet/fleet/race ref or sentinel).
/// `payload_hex` is the full record payload — parse remaining events
/// downstream once the per-arg-type widths are nailed down.
#[derive(Debug, Serialize)]
pub struct EventsRecord {
    /// Low-10-bit message ID parsed from bytes 0-1 of the record.
    pub header_msg_id: u16,
    /// High-6-bit tag from bytes 0-1 (purpose TBD; non-zero on some events).
    pub header_msg_tag: u16,
    /// u16 LE at bytes 2-3.  Planet/fleet/race ref, or sentinel
    /// (0xFFFF/0xFFFE/0xFFFC).
    pub header_loc: u16,
    /// Format string of the header message, if a template is known.
    pub header_fmt: Option<String>,
    /// Number of `\X` arguments the header template expects.
    pub header_args: Option<u8>,
    /// Full raw payload as lowercase hex (no separators).
    pub payload_hex: String,
}

/// Decode a type-12 EventsBlock payload into a partially-parsed record.
pub fn decode_events_block(payload: &[u8]) -> EventsRecord {
    let header_word = if payload.len() >= 2 {
        u16::from_le_bytes([payload[0], payload[1]])
    } else {
        0
    };
    let header_loc = if payload.len() >= 4 {
        u16::from_le_bytes([payload[2], payload[3]])
    } else {
        0
    };
    let mid = header_word & 0x3FF;
    let tag = header_word >> 10;
    let tmpl = lookup(header_word);
    EventsRecord {
        header_msg_id: mid,
        header_msg_tag: tag,
        header_loc,
        header_fmt: tmpl.map(|t| t.fmt.clone()),
        header_args: tmpl.map(|t| t.args),
        payload_hex: payload.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_table_has_known_entries() {
        let t = template_table();
        // Spot-check a few well-known IDs.
        assert_eq!(t.get(&169).map(|x| x.args), Some(1)); // "Your home planet is \p"
        assert_eq!(t.get(&181).map(|x| x.args), Some(1)); // "\Z winner"
        assert_eq!(t.get(&182).map(|x| x.args), Some(0)); // "you have been declared the winner"
        assert!(t.get(&127).unwrap().fmt.starts_with("Tip:"));
    }

    #[test]
    fn decode_winner_event() {
        // Real bytes from 900_turns/Game.m1 type-12 record:
        //   b5 00 fc ff 02  → msg 181 with race-idx 2 winning.
        let r = decode_events_block(&[0xb5, 0x00, 0xfc, 0xff, 0x02]);
        assert_eq!(r.header_msg_id, 181);
        assert_eq!(r.header_msg_tag, 0);
        assert_eq!(r.header_loc, 0xfffc);
        assert_eq!(r.header_args, Some(1));
    }

    #[test]
    fn decode_homeworld_event() {
        // Real bytes from original/Game.m1 (player 1, homeworld idx 68):
        //   a9 00 44 00 44  → msg 169 with planet idx 68.
        let r = decode_events_block(&[0xa9, 0x00, 0x44, 0x00, 0x44]);
        assert_eq!(r.header_msg_id, 169);
        assert_eq!(r.header_loc, 0x0044);
        assert_eq!(r.header_args, Some(1));
    }
}
