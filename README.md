# stars-file-parser

Import utilities for Stars! binary game files.

## Tools

| Binary | Input | Description |
|--------|-------|-------------|
| `r1_to_json` | `.r1` race file | Converts a Stars! race file to `race.json` |
| `m1_to_json` | `.m1` turn file | Extracts player turn state from a Stars! player file |
| `xy_to_json` | `.xy` universe file | Decodes universe parameters from a Stars! map file |

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
  "lrts": ["IFE", "TT"],
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

## m1_to_json

Reads a Stars! `.m1` player turn file and writes turn state JSON to stdout.
Works on any `.mN` file (one per player); pass the file path directly.

```sh
m1_to_json <file.m1>
```

### Output format

The output is a single JSON object with the following top-level keys:

```json
{
  "year": 2401,
  "player": { ... },
  "planets": [ ... ],
  "fleets": [ ... ],
  "waypoints": [ ... ]
}
```

**`player`** — state from the type-6 PlayerBlock:

| Field | Description |
|-------|-------------|
| `homeworld_planet_idx` | 0-based planet index of the player's homeworld |
| `tech_energy` … `tech_biology` | Current tech levels in all six fields |
| `race` | Race design parameters (same layout as `r1_to_json` output, minus name/icon) |
| `battle_plans` | List of battle plan records (type-30 BattlePlanBlock) |

**`planets`** — one entry per type-13 PlanetBlock visible to the player:

| Field | Description |
|-------|-------------|
| `planet_index` | 0-based index |
| `owner_id` | Player index (31 = uninhabited) |
| `colonized` | True if the planet has a colony |
| `conc_ironium/boranium/germanium` | Mineral concentrations (0–100) |
| `gravity/temperature/radiation` | Environment indices (0–100) |
| `surface_ironium/boranium/germanium` | Surface minerals in kT (colonised only) |
| `population` | Population in units of 100 colonists (colonised only) |
| `mines/factories/defenses` | Installation counts (colonised only) |

**`fleets`** — one entry per type-16 FleetBlock:

| Field | Description |
|-------|-------------|
| `fleet_index` | Fleet identifier |
| `orbit_planet_idx` | Orbited planet (65535 = en route) |
| `en_route` | True when the fleet is between planets |
| `x`, `y` | Coordinates in light-years |
| `design_bitmask` | Bitmask of design slots present in the fleet |
| `num_stacks` | Number of distinct ship designs |
| `ship_count` | Total ships |
| `total_mass_kt` | Total fleet mass in kT |
| `battle_plan_idx` | Active battle plan (0-based; omitted for simple fleets) |
| `waypoint_count` | Number of waypoints including current position |

**`waypoints`** — one entry per type-19/20 waypoint record, in file order
(each fleet's waypoints follow its type-16 record):

| Field | Description |
|-------|-------------|
| `x`, `y` | Waypoint coordinates |
| `planet_index` | Target planet (0 = deep space) |
| `warp_speed` | 0 = stationary/orbit, 1–10 = warp N |
| `task_type` | 0=None, 1=Transport, 2=Colonize, 3=Remote Mining, 4=Merge, 5=Scrap, 6=Lay Mines, 7=Patrol, 8=Route, 9=Transfer |
| `is_current_position` | True for the implicit first waypoint of an en-route fleet |
| `has_task_payload` | True for type-19 records (task parameters present) |
| `transport_payload` | Ironium/boranium/germanium/colonists/fuel ops (Transport task only) |

Each `transport_payload` resource op has `amount` (kT) and `action`
(0=No change, 1=Unload All, 2=Load All, 3=Load Exactly, 4=Unload Exactly, 5=Fill Up To).

### Known limitations

- **Planet records**: simplified decoding only works correctly for non-depleted,
  non-terraformed planets. Variable-length DepletionLength/SurfaceLength parsing
  is not fully implemented.
- **Fleet name**: encoding not yet reverse-engineered; name is not emitted.
- **Waypoint task payload**: only Transport (task_type=1) is decoded. Remote Mining,
  Patrol, and other task types produce no `transport_payload`.

## xy_to_json

Reads a Stars! `.xy` universe file and writes universe parameters to stdout.

```sh
xy_to_json <file.xy>
```

### Output format

```json
{
  "format_version": 1,
  "map_size": "medium",
  "planet_count": 288,
  "intended_player_count": 6,
  "difficulty": "easy|standard"
}
```

| Field | Values |
|-------|--------|
| `map_size` | `tiny` / `small` / `medium` / `large` / `huge` |
| `planet_count` | 32 / 128 / 288 / 512 / 800 for the five sizes |
| `intended_player_count` | Total players the game was set up with (human + AI) |
| `difficulty` | `easy\|standard` / `harder` / `expert` |

### Known limitations

- **Easy vs Standard**: both produce an identical `.xy` signature and cannot be
  distinguished from the map file alone.
- **`intended_player_count`** may exceed the number of `.m` files actually generated:
  harder/expert games drop players whose homeworlds cannot be placed at minimum spacing.

## File format background

Stars! data files are record containers: a sequence of 2-byte little-endian
headers (`high 6 bits = type`, `low 10 bits = payload length`) followed by
payloads.  Type-8 records are plaintext file headers that seed an L'Ecuyer
(1988) combined LCG; all other payloads are XOR-encrypted with the key stream
produced by that LCG.

The shared cipher and record-parsing logic lives in `src/cipher.rs` and
`src/records.rs` so that all parsers can reuse it.

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
| 78–79 | LRT bitmask (16-bit LE; bits 0–13 map to IFE, TT, ARM, ISB, GR, UR, MA, NRE, CE, OBRM, NAS, LSP, BET, RS) |
| 80 | Icon index (low 5 bits) |
| 81 | Flags: bit 7 = cheap germanium, bit 5 = expensive tech boost |
| 112+ | Name section |

Bytes 16–81 have the **same layout** in `.m` player turn files (type-6 PlayerBlock),
allowing `m1_to_json` to reuse the same race decoder.

Full layout specification: `stars-reborn-design/docs/new_game/race_file_format.rst`.

## Preset race names

The six default Stars! races use a preset name encoding (marker bytes 6 or 7
in the name section).  The lookup table in `r1_to_json.rs` covers all six
singular and plural forms.  User-typed race names are decoded directly from
the payload using a `byte − 111` character offset.
