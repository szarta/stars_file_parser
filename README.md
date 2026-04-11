# stars-file-parser

Import utilities for Stars! binary game files.

## Tools

| Binary | Input | Description |
|--------|-------|-------------|
| `r1_to_json` | `.r1` race file | Converts a Stars! race file to `race.json` |

More parsers (`.m` turn files, `.hst` host files, `.xy` universe files) will be added here as they are developed.

## Build

```sh
cargo build --release
```

Binaries land in `target/release/`.

## r1_to_json

Reads a Stars! `.r1` binary race file and writes an equivalent `race.json` to stdout.

```sh
r1_to_json <file.r1>
```

### Output format

The output is a single JSON object conforming to the race schema defined in
`stars-reborn-design/docs/new_game/race_file_format.rst`.

```json
{
  "format_version": 1,
  "name": "Humanoid",
  "plural_name": "Humanoids",
  "prt": "JOAT",
  "lrts": [],
  "hab": {
    "gravity":     { "immune": false, "min": 0.22, "max": 4.4  },
    "temperature": { "immune": false, "min": -140.0, "max": 140.0 },
    "radiation":   { "immune": false, "min": 15.0, "max": 85.0 }
  },
  "economy": {
    "resource_production": 1000,
    "factory_production": 10,
    "factory_cost": 10,
    "factory_cheap_germanium": false,
    "colonists_operate_factories": 10,
    "mine_production": 10,
    "mine_cost": 5,
    "colonists_operate_mines": 10,
    "growth_rate": 15
  },
  "research_costs": {
    "energy": "normal",
    "weapons": "normal",
    "propulsion": "normal",
    "construction": "normal",
    "electronics": "normal",
    "biotechnology": "normal"
  },
  "icon_index": 0
}
```

### Hab units

| Axis | Unit | Notes |
|------|------|-------|
| Gravity | g | Converted via the 101-entry Gravity_Map lookup table |
| Temperature | °C | Linear: `(index − 50) × 4`; range −200 to +200 |
| Radiation | mR/yr | Direct 0–100 integer |

### Known limitations

- **`lrts` is always `[]`** — bytes 78-79 of the payload encode LRTs and the
  icon index, but the exact bit layout is not yet reverse-engineered (research
  task R1.2 in `stars-reborn-research/PLAN.md`).
- **`icon_index` is always `0`** — same reason.

## File format background

Stars! data files are record containers: a sequence of 2-byte little-endian
headers (`high 6 bits = type`, `low 10 bits = payload length`) followed by
payloads.  Type-8 records are plaintext file headers that seed an L'Ecuyer
(1988) combined LCG; all other payloads are XOR-encrypted with the key stream
produced by that LCG.

The shared cipher and record-parsing logic lives in `src/cipher.rs` and
`src/records.rs` so that future parsers (`.m`, `.hst`, etc.) can reuse it.

The `.r1` type-6 payload is a direct memory dump of a 192-byte struct.
Confirmed field offsets:

| Offset | Field |
|--------|-------|
| 16–18 | Hab center (grav, temp, rad) — `0xFF` = immune |
| 19–21 | Hab min (grav, temp, rad) |
| 22–24 | Hab max (grav, temp, rad) |
| 25 | Growth rate (%) |
| 62 | Resource production ÷ 100 |
| 63 | Factory production |
| 64 | Factory cost |
| 65 | Colonists to operate factories (thousands) |
| 66 | Mine production |
| 67 | Mine cost |
| 68 | Colonists to operate mines (thousands) |
| 70–75 | Research costs: energy, weapons, propulsion, construction, electronics, biotechnology (0=Expensive, 1=Normal, 2=Cheap) |
| 76 | PRT (HE=0, SS=1, WM=2, CA=3, IS=4, SD=5, PP=6, IT=7, AR=8, JOAT=9) |
| 78–79 | LRT bitmask + icon index — **encoding unconfirmed** |
| 81 | Flags: bit 7 = cheap germanium, bit 5 = expensive tech boost |
| 112+ | Name section |

Full layout specification: `stars-reborn-design/docs/new_game/race_file_format.rst`.

## Preset race names

The six default Stars! races use a preset name encoding (marker bytes 6 or 7
in the name section).  The lookup table in `r1_to_json.rs` covers all six
singular and plural forms.  User-typed race names are decoded directly from
the payload using a `byte − 111` character offset.
