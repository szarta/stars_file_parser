@AGENTS.md

# stars-file-parser — Technical Guide

A standalone Rust library and set of CLI tools for reading (and in a few cases
writing) the binary and text file formats of the original Stars! (1995/1996).
It is useful on its own to anyone studying those formats, and is consumed by
the Stars Reborn clone only where the clone imports original files.

Read the relevant `docs/adr/` record before changing a settled policy it
covers.

## Layout

| Path                                                                     | Holds                                                                  |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `src/lib.rs`                                                             | Library entry; re-exports the decode modules                           |
| `src/records.rs`                                                         | The shared record-container framing used by `.mN`/`.hst`/`.xN`         |
| `src/race.rs` · `name.rs` · `cipher.rs`                                  | Race decode, the Stars! name encoding, the file cipher                 |
| `src/messages.rs` · `advantage_points.rs` · `gamedef.rs` · `universe.rs` | Per-domain decoders                                                    |
| `src/bin/*.rs`                                                           | One CLI per format (`r1_to_json`, `m1_to_json`, `x1_to_json`, …)       |
| `data/`                                                                  | Reference corpus (e.g. the message-template table); formatters skip it |

## Decode philosophy

- **Decode faithfully to the on-disk layout; never guess** (ADR-0001). When a
  field's meaning or width is not yet confirmed, surface the raw bytes (hex)
  rather than inventing a decoded value — a wrong decode is worse than an
  honest raw one, because a downstream consumer cannot tell it is wrong.
- **One source string per parse.** A caller that re-reads or re-slices the
  source instead of using what the parser returns desyncs byte offsets; pass
  the parser's own output through.

## Checks

`pre-commit run --all-files` runs rustfmt, clippy (`-D warnings`), `cargo
test`, moldy (JSON/YAML/Markdown), the knots complexity gate, and the
commit-message policy. Install with `pre-commit install`.
