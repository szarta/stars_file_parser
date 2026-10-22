# Architectural Decision Records

Settled policy questions about stars-file-parser: the ones likely to come up
again as "should we change this?" in a later session that lacks the context of
the one that settled them. Read the relevant ADR before proposing to change
what it covers.

## When to write one

When a question is settled that isn't obvious from the code, isn't already
written in the README or `CLAUDE.md`, and is likely to be asked again. A commit
message describes one piece of work; an ADR records a standing policy that
should outlive it.

## Format

`NNNN-short-slug.md`, numbered in sequence, never renumbered or deleted. Each
file has Status (Accepted, with date), Context, Decision, and Consequences. A
later ADR may supersede an earlier one; the earlier one stays as the record
with a pointer to its replacement, instead of being edited in place.

## Index

- [0001: Decode faithfully to the on-disk layout; surface unknown bytes raw
  rather than guess](0001-decode-faithfully-surface-unknown-bytes-raw.md)
