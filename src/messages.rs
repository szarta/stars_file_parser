// EventsBlock (record type-12) decoding.
//
// One type-12 record holds the player's whole per-turn message log: a run of
// variable-length events, each referencing a message template (0..386) from
// Wasp's 2010 HWF table (shipped as ``data/messages.json``) plus argument bytes
// that fill the template's `\p`, `\s`, `\i`, … format codes.
//
// Per-event layout
// ----------------
//   * Bytes 0-1 (LE u16) = ``msg_word``:
//       - bits 0-8  (``& 0x1FF``) = message-template ID (0..386),
//       - bit  9     (``0x200``)  = WIDE-REFERENCE flag: when set, object-ref
//         arguments (`\p`) are 2 bytes instead of 1 — this is how maps with
//         >256 planets widen the ref, NOT a separate leading type byte,
//       - bits 10-15 (``>> 10``)  = a tag whose meaning is still unknown
//         (seen = 1 on msg 345).
//     This 9-bit split is confirmed by the large-map homeworld sample
//     ``a9 02 15 01 15 01``: ``0x02a9 & 0x1FF`` = 169 (home-planet message) with
//     the wide-ref bit set and a 2-byte planet ref 0x0115 = 277, whereas the
//     old 10-bit mask gave the out-of-range id 681.
//   * Bytes 2-3 (LE u16) = ``loc``: the object the message is bound to (planet
//     idx, fleet/race ref) or a sentinel when it is not — ``0xFFFF`` (tips),
//     ``0xFFFE`` (research / non-located), ``0xFFFC`` (winner declarations).
//     Always 2 bytes.
//   * Then one value per `\X` arg code in the template, in order.
//
// Per-arg byte widths (confirmed)
// -------------------------------
//   \p (planet)     = 1 byte, or 2 when the wide-reference bit is set.
//   \Z (race idx)   = 1 byte.
//   \j (tech field) = 1 byte.
// Every other code (\i, \s, \n, \l, \L, \f, \F, \k, \m, \v00, \vkT, \img, …) has
// an unconfirmed width — \i alone is 1 byte in some messages (tech level, msg
// 80) and 2 in others (resources, msg 345), so widths cannot be inferred from
// the format string without a varied oracle corpus (see the follow-up task and
// knowledge/findings/events_block.rst).
//
// The walker therefore decodes every event's 4-byte header (always reliable)
// and consumes arguments while the codes have confirmed widths; the first time
// it meets an unknown-width code it stops and surfaces the remaining bytes as a
// raw tail rather than guessing a boundary (ADR-0001). Turn-start logs (tips +
// home planet + production-queue-empty, all ARGS=0 or `\p`) decode in full.

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

/// Mask for the 9-bit message-template id within a ``msg_word``.
const MSG_ID_MASK: u16 = 0x1FF;
/// Wide-reference flag (bit 9): object-ref args (`\p`) are 2 bytes when set.
const WIDE_REF_FLAG: u16 = 0x200;

/// Look up a message template by a raw ``msg_word``: the 9-bit id is extracted
/// (the wide-ref flag and the high tag are stripped) before the lookup.
pub fn lookup(raw_mid: u16) -> Option<&'static MessageTemplate> {
    template_table().get(&(raw_mid & MSG_ID_MASK))
}

/// Byte width of one `\X` argument, given the event's wide-reference flag.
/// Returns `None` for any code whose width is not yet confirmed, which stops
/// the walker (see module docs).
fn arg_width(code: &str, wide_ref: bool) -> Option<usize> {
    match code {
        "\\p" => Some(if wide_ref { 2 } else { 1 }), // planet idx (widens on >256-planet maps)
        "\\Z" => Some(1),                            // race idx
        "\\j" => Some(1),                            // tech field (0-5)
        _ => None,                                   // unconfirmed width
    }
}

/// Extract the template's `\X` format codes in order (each as e.g. `"\\p"`).
/// A multi-character code such as `\v00` is returned as its backslash+letter
/// stem (`"\\v"`); since every such code currently has an unknown width that is
/// enough for the walker to stop on it.
fn arg_codes(fmt: &str) -> Vec<String> {
    let bytes = fmt.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic() {
            out.push(format!("\\{}", bytes[i + 1] as char));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn read_uint_le(p: &[u8], off: usize, width: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..width {
        v |= (p[off + i] as u32) << (8 * i);
    }
    v
}

/// One decoded argument of an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventArg {
    /// The format code this value filled, e.g. `"\\p"`.
    pub code: String,
    /// Decoded value (planet idx / race idx / tech field).
    pub value: u32,
    /// Number of bytes consumed.
    pub bytes: u8,
}

/// One decoded event within a type-12 record.
#[derive(Debug, Serialize)]
pub struct DecodedEvent {
    /// 9-bit message-template id (bytes 0-1 `& 0x1FF`).
    pub msg_id: u16,
    /// High tag (bits 10-15); meaning unknown, surfaced for research.
    pub msg_tag: u16,
    /// Wide-reference flag (bit 9): object-ref args are 2 bytes.
    pub wide_ref: bool,
    /// u16 LE at bytes 2-3: bound object idx or a sentinel
    /// (0xFFFF/0xFFFE/0xFFFC).
    pub loc: u16,
    /// Template format string, if the id is known.
    pub fmt: Option<String>,
    /// Number of `\X` args the template declares.
    pub args_expected: Option<u8>,
    /// Arguments decoded with confirmed widths (may be a prefix of all args).
    pub args: Vec<EventArg>,
    /// True when the id is known and every one of its args was decoded; false
    /// when an unknown-width code (or a short buffer) stopped decoding — the
    /// undecoded bytes from this event onward are in `EventsRecord.tail_hex`.
    pub fully_decoded: bool,
}

/// One EventsBlock (type-12) record: the walked event list plus, if the walk
/// stopped early, the raw bytes it could not resolve.
#[derive(Debug, Serialize)]
pub struct EventsRecord {
    /// Events decoded in file order. The last entry has `fully_decoded = false`
    /// when the walk stopped inside it.
    pub events: Vec<DecodedEvent>,
    /// Bytes from the first unresolved argument onward, as lowercase hex;
    /// `None` when the whole record decoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail_hex: Option<String>,
    /// Full raw payload as lowercase hex (no separators).
    pub payload_hex: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a type-12 EventsBlock payload by walking its event list.
pub fn decode_events_block(payload: &[u8]) -> EventsRecord {
    let mut events = Vec::new();
    let mut tail_hex = None;
    let mut off = 0usize;

    while off + 4 <= payload.len() {
        let msg_word = u16::from_le_bytes([payload[off], payload[off + 1]]);
        let loc = u16::from_le_bytes([payload[off + 2], payload[off + 3]]);
        let msg_id = msg_word & MSG_ID_MASK;
        let wide_ref = msg_word & WIDE_REF_FLAG != 0;
        let msg_tag = msg_word >> 10;
        let tmpl = template_table().get(&msg_id);

        // Consume one value per format code while widths are known.
        let codes = tmpl.map(|t| arg_codes(&t.fmt)).unwrap_or_default();
        let mut args = Vec::new();
        let mut cur = off + 4;
        let mut ok = tmpl.is_some();
        if ok {
            for code in &codes {
                match arg_width(code, wide_ref) {
                    Some(w) if cur + w <= payload.len() => {
                        args.push(EventArg {
                            code: code.clone(),
                            value: read_uint_le(payload, cur, w),
                            bytes: w as u8,
                        });
                        cur += w;
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
        }

        events.push(DecodedEvent {
            msg_id,
            msg_tag,
            wide_ref,
            loc,
            fmt: tmpl.map(|t| t.fmt.clone()),
            args_expected: tmpl.map(|t| t.args),
            args,
            fully_decoded: ok,
        });

        if !ok {
            // Unknown-width code (or unknown id): the boundary after `cur` is
            // unknown, so stop and surface the remainder rather than guess.
            tail_hex = Some(to_hex(&payload[cur..]));
            break;
        }
        off = cur;
    }

    // Trailing bytes too short to form a header (padding / unresolved).
    if tail_hex.is_none() && off < payload.len() {
        tail_hex = Some(to_hex(&payload[off..]));
    }

    EventsRecord {
        events,
        tail_hex,
        payload_hex: to_hex(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_table_has_known_entries() {
        let t = template_table();
        assert_eq!(t.get(&169).map(|x| x.args), Some(1)); // "Your home planet is \p"
        assert_eq!(t.get(&181).map(|x| x.args), Some(1)); // "\Z winner"
        assert_eq!(t.get(&182).map(|x| x.args), Some(0)); // "you have been declared the winner"
        assert!(t.get(&127).unwrap().fmt.starts_with("Tip:"));
    }

    #[test]
    fn walks_turn_one_log_fully() {
        // Real type-12 payload from experiments/Game.m1: four ARGS=0 tips
        // (127-130) then the home-planet message (169, \p) for planet 111.
        let payload = [
            0x7f, 0x00, 0xff, 0xff, // 127 tip
            0x80, 0x00, 0xff, 0xff, // 128 tip
            0x81, 0x00, 0xff, 0xff, // 129 tip
            0x82, 0x00, 0xff, 0xff, // 130 tip
            0xa9, 0x00, 0x6f, 0x00, 0x6f, // 169 home planet 111
        ];
        let r = decode_events_block(&payload);
        assert_eq!(r.events.len(), 5);
        assert!(r.tail_hex.is_none(), "turn-1 log decodes completely");
        assert!(r.events.iter().all(|e| e.fully_decoded));
        let home = &r.events[4];
        assert_eq!(home.msg_id, 169);
        assert!(!home.wide_ref);
        assert_eq!(home.loc, 111);
        assert_eq!(home.args, vec_arg("\\p", 111, 1));
    }

    #[test]
    fn large_map_home_planet_uses_the_wide_reference_flag() {
        // experiments/.../medium_dense_selected: same four tips, then the home
        // planet encoded a9 02 15 01 15 01 — bit 9 set, 2-byte planet ref 277.
        let payload = [
            0x7f, 0x00, 0xff, 0xff, 0x80, 0x00, 0xff, 0xff, 0x81, 0x00, 0xff, 0xff, 0x82, 0x00,
            0xff, 0xff, 0xa9, 0x02, 0x15, 0x01, 0x15, 0x01,
        ];
        let r = decode_events_block(&payload);
        assert_eq!(r.events.len(), 5);
        assert!(r.tail_hex.is_none());
        let home = &r.events[4];
        assert_eq!(home.msg_id, 169, "9-bit mask recovers the id past the flag");
        assert!(home.wide_ref, "bit 9 set");
        assert_eq!(home.loc, 277);
        assert_eq!(
            home.args,
            vec_arg("\\p", 277, 2),
            "planet ref widened to 2 bytes"
        );
    }

    #[test]
    fn decodes_winner_declaration() {
        // b5 00 fc ff 02 → msg 181, \Z race idx 2.
        let r = decode_events_block(&[0xb5, 0x00, 0xfc, 0xff, 0x02]);
        assert_eq!(r.events.len(), 1);
        let e = &r.events[0];
        assert_eq!(e.msg_id, 181);
        assert_eq!(e.msg_tag, 0);
        assert_eq!(e.loc, 0xfffc);
        assert!(e.fully_decoded);
        assert_eq!(e.args, vec_arg("\\Z", 2, 1));
        assert!(r.tail_hex.is_none());
    }

    #[test]
    fn stops_at_an_unknown_width_code_and_keeps_the_tail() {
        // 50 00 fe ff 0d 01 02 → msg 80 "Tech Level \i for \j…"; \i width is
        // unconfirmed, so the walk stops before the args and banks the tail.
        let r = decode_events_block(&[0x50, 0x00, 0xfe, 0xff, 0x0d, 0x01, 0x02]);
        assert_eq!(r.events.len(), 1);
        let e = &r.events[0];
        assert_eq!(e.msg_id, 80);
        assert_eq!(e.loc, 0xfffe);
        assert!(!e.fully_decoded, "\\i has no confirmed width");
        assert!(e.args.is_empty());
        assert_eq!(r.tail_hex.as_deref(), Some("0d0102"));
    }

    fn vec_arg(code: &str, value: u32, bytes: u8) -> Vec<EventArg> {
        vec![EventArg {
            code: code.to_string(),
            value,
            bytes,
        }]
    }
}
