# 0001: Decode faithfully to the on-disk layout; surface unknown bytes raw rather than guess

**Status:** Accepted, 2026-10-22.

## Context

This crate reverse-engineers undocumented binary formats. Much is confirmed by
oracle work, but parts of some records are not yet understood — a field's
width varies with map size, a record type has a form that has not been decoded,
or high bits of a value have an unknown purpose. There is a standing
temptation, when a field is "probably" an integer or an index, to decode it to
a plausible value so the JSON looks complete.

## Decision

Decode only what is confirmed. Where a field's meaning, width, or presence is
not established, surface the raw bytes (as hex, or as an explicitly-named raw
payload) instead of a guessed value. Comment the record with what _is_ known
and what is not. A JSON field never presents an unconfirmed guess as if it were
decoded.

## Consequences

- A consumer can always tell confirmed data from not-yet-decoded data; a wrong
  decode is worse than an honest raw one, because the consumer cannot detect
  the error, whereas a raw payload is self-evidently undecoded.
- Progress is additive: when a form is later decoded, the raw payload is
  replaced by named fields, and nothing that depended on a guess has to be
  unwound.
- The JSON output can carry both a decoded view and the raw bytes for a record
  still being studied, which is the intended state during reverse engineering.
