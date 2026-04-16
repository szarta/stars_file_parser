# stars-file-parser

Import utilities for Stars! binary game files.

## Tools

| Binary | Input | Description |
|--------|-------|-------------|
| `r1_to_json` | `.r1` race file | Converts a Stars! race file to `race.json` |
| `json_to_r1` | `race.json` | Writes a Stars!-loadable `.r1` from a race JSON |
| `m1_to_json` | `.m1` turn file | Extracts player turn state from a Stars! player file |
| `xy_to_json` | `.xy` universe file | Decodes universe parameters from a Stars! map file |
| `json_to_def` | `gamedef.json` | Writes a `.def` file for `stars.exe -a` universe creation |

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

## json_to_def

Reads a game definition JSON and writes a Stars! `.def` file for use with
`stars.exe -a`.

```sh
json_to_def <input.json | -> <output.def | ->
```

Use `-` for stdin/stdout.  The output is a Windows CRLF text file.  Stars!
silently ignores `.def` files with LF-only endings and creates no output.

### Workflow

```bash
# 1. Create a race JSON with r1_to_json (or write one by hand)
r1_to_json myrace.r1 > myrace.json

# 2. Write a game definition JSON (see schema below)
# 3. Generate the .def
json_to_def game.json game.def

# 4. Create the universe (requires Xvfb if no display)
Xvfb :99 -screen 0 1024x768x24 &
DISPLAY=:99 WINEPREFIX=~/.wine32 WINEARCH=win32 \
  wine /path/to/stars.exe -a game.def

# 5. Generate turn 1
DISPLAY=:99 WINEPREFIX=~/.wine32 WINEARCH=win32 \
  wine /path/to/stars.exe -g1 GameName.hst

# 6. Parse the result
m1_to_json GameName.m1
```

### Input JSON schema

```json
{
  "game_name": "TestGame",
  "universe": {
    "map_size": "small",
    "density": "normal",
    "player_positions": "farther",
    "seed": 12345
  },
  "options": {
    "max_minerals": false,
    "slow_tech": false,
    "bbs_play": false,
    "galaxy_clumping": false,
    "computer_alliances": false,
    "no_random_events": false,
    "public_scores": false
  },
  "players": [
    { "human": { "race_file": "Z:\\path\\to\\race.r1" } },
    { "ai":    { "difficulty": 2, "param": 1 } }
  ],
  "victory": {
    "planets":         { "enabled": true,  "percent":  60  },
    "tech":            { "enabled": true,  "level":    26, "fields": 4 },
    "score":           { "enabled": false, "score":    5000 },
    "exceeds_nearest": { "enabled": false, "percent":  150 },
    "production":      { "enabled": false, "capacity": 100 },
    "capital_ships":   { "enabled": false, "number":   100 },
    "turns":           { "enabled": false, "years":    100 },
    "must_meet": 1,
    "min_years": 50
  },
  "output_xy": "Z:\\path\\to\\output.xy"
}
```

**`universe` fields:**

| Field | Values |
|-------|--------|
| `map_size` | `tiny` / `small` / `medium` / `large` / `huge` |
| `density` | `sparse` / `normal` / `dense` / `packed` |
| `player_positions` | `close` / `moderate` / `farther` / `distant` |
| `seed` | Any 32-bit integer; determines the universe layout reproducibly |

**`players`:** each element is either `{ "human": { "race_file": "..." } }` or
`{ "ai": { "difficulty": N, "param": 1 } }` where `difficulty` is
0=easy / 1=standard / 2=harder / 3=expert.  The `param` field is always `1`
in all observed oracle files; its purpose is not yet confirmed.

**Victory condition fields:** each VC has `enabled` plus the condition-specific
value.  `must_meet` is how many simultaneously-satisfied conditions trigger a
win; `min_years` is the minimum game length in years before any win can occur.

### Known limitations

- **`player_positions` integer mapping**: Close=0, Moderate=1, Farther=2,
  Distant=3 is inferred from the Stars! UI order; only `farther` (2) has been
  oracle-tested.
- **AI `param` field**: always written as `1`; actual effect is unknown.
- **Output file location**: `.hst` and `.mN` files are always written to the
  CWD where `stars.exe` runs, regardless of the `output_xy` path.

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
