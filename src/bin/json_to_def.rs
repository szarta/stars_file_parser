// json_to_def — read a game definition JSON and write a Stars! .def file.
//
// Usage:
//   json_to_def <input.json>  <output.def>
//   json_to_def <input.json>  -          (write to stdout)
//   json_to_def -             <output.def>  (read from stdin)
//
// The input JSON must conform to the GameDef schema defined in gamedef.rs.
// The output is a Windows CRLF text file suitable for `stars.exe -a`.
//
// Example input:
//
//   {
//     "game_name": "TestGame",
//     "universe": {
//       "map_size": "small",
//       "density": "normal",
//       "player_positions": "farther",
//       "seed": 12345
//     },
//     "options": {
//       "max_minerals": false,
//       "slow_tech": false,
//       "bbs_play": false,
//       "galaxy_clumping": false,
//       "computer_alliances": false,
//       "no_random_events": false,
//       "public_scores": false
//     },
//     "players": [
//       { "human": { "race_file": "Z:\\path\\to\\race.r1" } },
//       { "ai":    { "difficulty": 2, "param": 1 } }
//     ],
//     "victory": {
//       "planets":          { "enabled": true,  "percent":  60  },
//       "tech":             { "enabled": true,  "level":    26, "fields": 4 },
//       "score":            { "enabled": false, "score":    5000 },
//       "exceeds_nearest":  { "enabled": false, "percent":  150 },
//       "production":       { "enabled": false, "capacity": 100 },
//       "capital_ships":    { "enabled": false, "number":   100 },
//       "turns":            { "enabled": false, "years":    100 },
//       "must_meet": 1,
//       "min_years": 50
//     },
//     "output_xy": "Z:\\path\\to\\output.xy"
//   }

use std::{env, fs, io::{self, Write}, path::Path, process};

use stars_file_parser::gamedef::{to_def_bytes, GameDef};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: json_to_def <input.json | -> <output.def | ->");
        process::exit(1);
    }

    // Read input JSON.
    let json_text = if args[1] == "-" {
        io::read_to_string(io::stdin()).unwrap_or_else(|e| {
            eprintln!("error reading stdin: {e}");
            process::exit(1);
        })
    } else {
        fs::read_to_string(Path::new(&args[1])).unwrap_or_else(|e| {
            eprintln!("error reading {}: {e}", args[1]);
            process::exit(1);
        })
    };

    // Parse game definition.
    let def: GameDef = serde_json::from_str(&json_text).unwrap_or_else(|e| {
        eprintln!("JSON parse error: {e}");
        process::exit(1);
    });

    if def.players.is_empty() {
        eprintln!("error: players list is empty");
        process::exit(1);
    }

    // Serialise to .def bytes.
    let def_bytes = to_def_bytes(&def);

    // Write output.
    if args[2] == "-" {
        io::stdout().write_all(&def_bytes).unwrap_or_else(|e| {
            eprintln!("error writing stdout: {e}");
            process::exit(1);
        });
    } else {
        fs::write(Path::new(&args[2]), &def_bytes).unwrap_or_else(|e| {
            eprintln!("error writing {}: {e}", args[2]);
            process::exit(1);
        });
    }
}
