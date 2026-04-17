# stars-file-parser

Import and inspection utilities for Stars! game files — binary player files,
race files, and plain-text oracle dumps.

## Tools

| Binary | Input | Description |
|--------|-------|-------------|
| `r1_to_json` | `.r1` race file | Converts a Stars! race file to JSON |
| `json_to_r1` | `race.json` | Writes a Stars!-loadable `.r1` from a race JSON |
| `m1_to_json` | `.mN` turn file | Extracts full player turn state from a Stars! player file |
| `xy_to_json` | `.xy` universe file | Decodes universe parameters from a Stars! map file |
| `json_to_def` | `gamedef.json` | Writes a `.def` file for `stars.exe -a` universe creation |
| `map_to_json` | `.map` universe dump | Parses a Stars! universe text dump to JSON |
| `pla_to_json` | `.pla` / `.pNN` planet dump | Parses a Stars! planet text dump to JSON (basic or rich) |
| `fle_to_json` | `.fle` / `.fNN` fleet dump | Parses a Stars! fleet text dump to JSON (basic or rich) |
| `dump_records` | any Stars! binary | Debug: print record types and lengths |
| `dump_type16` | `.mN` turn file | Debug: print raw type-16 FleetBlock payloads |

## Build

```sh
cargo build --release
```

Binaries land in `target/release/`.

---

## Text dump tools

Stars! can export human-readable text files via `stars.exe -d[pfm] <game>.mN`
(no display or Xvfb required — the `-d` flag is fully headless).  These three
tools parse those exports into JSON for oracle validation.

Before generating dumps, set `NewReports=1` in the `[Misc]` section of
`~/.wine32/drive_c/windows/Stars.ini` to get the richer `.pNN`/`.fNN` format.
Use `create_stars_ini.py` in `stars-reborn-research/automation/` to do this
idempotently, or run `dump_game_info.py` which calls it automatically.

### map_to_json

Parses a `.map` universe dump (produced by `stars.exe -dm`).  Lists every
planet in the universe with its position.  The `number` field is the 1-based
ordinal that matches planet ordering in the `.xy` file.

```sh
map_to_json <file.map>
```

```json
{
  "planets": [
    { "number": 1, "x": 1014, "y": 1047, "name": "Halsey" },
    { "number": 52, "x": 1310, "y": 1220, "name": "Sulfur" }
  ]
}
```

### pla_to_json

Parses a planet dump — either the basic `.pla` (20 columns) or the richer
`.pNN` (36 columns, `NewReports=1`).  Format is detected automatically from
the header row.

```sh
pla_to_json <file.pla|file.pNN>
```

**Basic format** (`"format": "basic"`) — present in both:

| Field | Description |
|-------|-------------|
| `planet_name` | Planet name |
| `owner` | Race name of the owner |
| `starbase_type` | `"Starbase"`, `"Orbital Fort"`, `"--"` = none |
| `report_age` | Turns since last observation; 0 = current turn |
| `population` | Colony population |
| `value_pct` | Habitability value as signed integer percent; null if unparseable |
| `production_queue` | Queue description string |
| `mines` / `factories` | Installation counts |
| `defense_pct` | Defense coverage as float (e.g. `9.56`) |
| `surface_iron/bora/germ` | Surface minerals in kT |
| `iron/bora/germ_mining_rate` | Mining rate per mineral |
| `iron/bora/germ_mineral_conc` | Mineral concentration (0–100) |
| `resources` | Resources generated per turn |

**Rich-only fields** (`"format": "rich"`, omitted in basic output):

| Field | Description |
|-------|-------------|
| `gravity` / `temperature` / `radiation` | Current hab as display string (e.g. `"1.00g"`, `"0°C"`, `"50mR"`) |
| `gravity_orig` / `temperature_orig` / `radiation_orig` | Original pre-terraform hab |
| `terraformed_pct` | 100 = not terraformed; lower = degree of terraforming |
| `capacity` | Raw `Cap` field; semantics to be confirmed vs. oracle |
| `scan_range` / `pen_scan_range` | Scanner ranges in ly |
| `driver` / `warp` | Mass driver target and warp speed (empty/0 = no driver) |
| `route` | Route destination (empty = no route) |
| `gate_range` / `gate_mass` | Stargate limits in ly/kT (0 = no gate) |
| `pct_damaged` | Percent of defenses damaged |

**Note:** the temperature string contains `°C` (CP1252 byte `0xB0`); under
`from_utf8_lossy` this appears as the Unicode replacement character `\u{FFFD}`.
This only affects display strings; all numeric fields parse correctly.

### fle_to_json

Parses a fleet dump — either the basic `.fle` (12 columns) or the richer
`.fNN` (29 columns, `NewReports=1`).  Format is detected automatically.

```sh
fle_to_json <file.fle|file.fNN>
```

**Basic format** (`"format": "basic"`) — present in both:

| Field | Description |
|-------|-------------|
| `fleet_name` | Fleet name (e.g. `"Humanoid Armed Probe #1"`) |
| `x` / `y` | Fleet coordinates |
| `planet` | Current location name; `"--"` if in deep space |
| `destination` | Next waypoint name; `"--"` if no orders |
| `battle_plan` | Active battle plan name |
| `ship_count` | Number of ships in the fleet |
| `cargo_iron/bora/germ` | Cargo in kT |
| `cargo_colonists` | Colonist cargo |
| `fuel` | Fuel on board in mg |

**Rich-only fields** (`"format": "rich"`, omitted in basic output):

| Field | Description |
|-------|-------------|
| `owner` | Player number (1-based) |
| `eta` | Turns to destination (0 = in orbit) |
| `warp` | Warp speed to destination (0 = not moving) |
| `mass_kt` | Total fleet mass in kT |
| `cloak_pct` | Cloaking level 0–100 |
| `scan_range` / `pen_scan_range` | Scanner ranges in ly |
| `task` | Current waypoint task (e.g. `"(no task here)"`, `"Transport"`) |
| `mining_rate` | Remote mining rate in kT/turn |
| `sweep_rate` | Mine sweep rate |
| `mine_laying_rate` | Mines laid per turn |
| `terraform_rate` | Terraforming rate |
| `ships_unarmed/scout/warship/utility/bomber` | Ship counts by combat role |

---

## Binary file tools

### r1_to_json

Reads a Stars! `.r1` binary race file and writes an equivalent race JSON to stdout.

```sh
r1_to_json <file.r1>
```

#### Output format

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

#### Hab units

| Axis | Unit | Notes |
|------|------|-------|
| Gravity | g | Converted via the 101-entry Gravity_Map lookup table |
| Temperature | °C | Linear: `(index − 50) × 4`; range −200 to +200 |
| Radiation | mR/yr | Direct 0–100 integer |

### m1_to_json

Reads a Stars! `.mN` player turn file and writes turn state JSON to stdout.

```sh
m1_to_json <file.m1>
```

#### Output format

```json
{
  "year": 2401,
  "player": { ... },
  "planets": [ ... ],
  "fleets": [ ... ],
  "waypoints": [ ... ],
  "designs": [ ... ]
}
```

**`player`** — state from the type-6 PlayerBlock:

| Field | Description |
|-------|-------------|
| `homeworld_planet_idx` | 0-based planet index of the player's homeworld |
| `planet_count` | Number of planets owned by this player |
| `tech_energy` … `tech_biology` | Current tech levels in all six fields |
| `race` | Race design parameters (same layout as `r1_to_json`, minus name/icon) |
| `battle_plans` | List of battle plan records (type-30 BattlePlanBlock) |

Each battle plan has `plan_id`, `tactic`, `primary_target`, `secondary_target`,
and `attack_who`.  Target codes: 0=None, 1=Any, 2=Starbase, 3=Armed Ships,
4=Bombers/Freighters, 5=Unarmed Ships, 6=Fuel Transports, 7=Freighters.

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
| `orbit_planet_idx` | Orbited planet index (65535 = en route) |
| `en_route` | True when the fleet is between planets |
| `x`, `y` | Coordinates |
| `design_bitmask` | Bitmask of design slots present in the fleet |
| `num_stacks` | Number of distinct ship designs |
| `b16_raw` | Raw byte 16; correlates with fuel > 255 (1 when fuel ≤ 255, 2 when fuel > 255) |
| `fuel_mg` | Fuel on board in mg (confirmed from in-game Report→Fleets, 2026-04-17) |
| `battle_plan_idx` | Active battle plan index (omitted for fleets with fuel ≤ 255) |
| `waypoint_count` | Number of waypoints including current position |

**`waypoints`** — one entry per type-19/20 waypoint record, in file order:

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

**`designs`** — one entry per type-26 DesignBlock (full-design records only):

| Field | Description |
|-------|-------------|
| `design_number` | Design slot index |
| `is_starbase` | True for orbital structures |
| `hull_id` / `hull_name` | Hull type numeric ID and name |
| `pic` | Picture index |
| `armor` | Total armor value |
| `slot_count` | Number of component slots |
| `turn_designed` | Turn the design was created |
| `total_built` / `total_remaining` | Lifetime production and surviving count |
| `name` | Design name (decoded from Stars! nibble encoding) |
| `slots` | List of component slots: `category`, `category_name`, `item_id`, `count`, `component` |

#### Known limitations

- **Planet records**: simplified decoding only works correctly for non-depleted,
  non-terraformed planets.
- **Fleet name**: not decoded (encoding not yet confirmed).
- **Waypoint task payload**: only Transport (task_type=1) is decoded.

### xy_to_json

Reads a Stars! `.xy` universe file and writes universe parameters to stdout.

```sh
xy_to_json <file.xy>
```

#### Output format

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
| `planet_count` | 32 / 128 / 288 / 512 / 800 |
| `intended_player_count` | Total players the game was set up with |
| `difficulty` | `easy\|standard` / `harder` / `expert` |

#### Known limitations

- Easy and Standard produce an identical `.xy` signature and cannot be distinguished.

### json_to_def

Reads a game definition JSON and writes a Stars! `.def` file for use with
`stars.exe -a`.

```sh
json_to_def <input.json> <output.def>
```

Use `-` for stdin/stdout.  The output uses Windows CRLF line endings — Stars!
silently ignores `.def` files with LF-only endings.

#### Workflow

```bash
# 1. Build a race JSON
r1_to_json myrace.r1 > myrace.json

# 2. Write a game definition JSON (see schema below)

# 3. Generate the .def
json_to_def game.json game.def

# 4. Create the universe (headless)
DISPLAY=:99 WINEPREFIX=~/.wine32 WINEARCH=win32 \
  wine /path/to/stars.exe -a game.def

# 5. Generate turn 1
DISPLAY=:99 WINEPREFIX=~/.wine32 WINEARCH=win32 \
  wine /path/to/stars.exe -g1 GameName.hst

# 6. Parse the result
m1_to_json GameName.m1

# 7. Dump oracle text files (no display needed)
WINEPREFIX=~/.wine32 WINEARCH=win32 DISPLAY=:0 \
  wine /path/to/stars.exe -dfmp GameName.m1
map_to_json GameName.map
pla_to_json GameName.p1
fle_to_json GameName.f1
```

#### Input JSON schema

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
| `seed` | Any 32-bit integer |

**`players`:** each element is `{ "human": { "race_file": "..." } }` or
`{ "ai": { "difficulty": N, "param": 1 } }` where `difficulty` is
0=easy / 1=standard / 2=harder / 3=expert.

#### Known limitations

- **`player_positions`**: only `farther` (2) has been oracle-tested.
- **AI `param` field**: always written as `1`; actual effect is unknown.
- **Output file location**: `.hst` and `.mN` files are written to the CWD
  where `stars.exe` runs, regardless of `output_xy`.

---

## File format background

Stars! binary data files (`.r1`, `.mN`, `.xN`, `.hst`, `.xy`) are record
containers: a sequence of 2-byte little-endian headers (`high 6 bits = type`,
`low 10 bits = payload length`) followed by payloads.  Type-8 records are
plaintext file headers that seed an L'Ecuyer (1988) combined LCG; all other
payloads are XOR-encrypted with the key stream from that LCG.

The shared cipher and record-parsing logic lives in `src/cipher.rs` and
`src/records.rs` so all parsers can reuse it.

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

Bytes 16–81 have the **same layout** in `.mN` player turn files (type-6 PlayerBlock),
allowing `m1_to_json` to reuse the same race decoder.

Full layout specification: `stars-reborn-design/docs/new_game/race_file_format.rst`.
