// map_to_json — parse a Stars! universe dump (.map) into JSON.
//
// Usage:  map_to_json <file.map>
//
// The .map file is produced by `stars.exe -dm <game>.mN` (or -dfmp for all
// three dump types).  It lists every planet in the universe with its position.
//
// Output: a JSON object with a single "planets" array, one entry per planet.
// The "number" field is the 1-based ordinal from the dump (matches planet
// ordering in the .xy universe file).
//
// File format (tab-separated, first line is header):
//   #   X   Y   Name
//   1   1014  1047  Halsey
//   ...

use std::{collections::HashMap, env, path::Path, process};

use serde::Serialize;

#[derive(Debug, Serialize)]
struct UniversePlanet {
    /// 1-based ordinal from the dump (matches .xy planet ordering).
    number: u32,
    x: i32,
    y: i32,
    name: String,
}

#[derive(Debug, Serialize)]
struct UniverseDump {
    planets: Vec<UniversePlanet>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: map_to_json <file.map>");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let raw = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    });
    let text = String::from_utf8_lossy(&raw);

    let mut lines = text.lines().peekable();

    // First non-empty line is the header.
    let header_line = loop {
        match lines.next() {
            Some(l) if !l.trim().is_empty() => break l,
            Some(_) => continue,
            None => {
                eprintln!("error: file is empty or has no header");
                process::exit(1);
            }
        }
    };

    let headers: HashMap<&str, usize> = header_line
        .split('\t')
        .enumerate()
        .map(|(i, h)| (h.trim(), i))
        .collect();

    for required in &["#", "X", "Y", "Name"] {
        if !headers.contains_key(required) {
            eprintln!("error: missing expected column '{}' in header", required);
            process::exit(1);
        }
    }

    let idx_num  = headers["#"];
    let idx_x    = headers["X"];
    let idx_y    = headers["Y"];
    let idx_name = headers["Name"];

    let mut planets: Vec<UniversePlanet> = Vec::new();

    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let get = |idx: usize| -> &str { fields.get(idx).copied().unwrap_or("").trim() };

        let number: u32 = get(idx_num).parse().unwrap_or(0);
        let x: i32      = get(idx_x).parse().unwrap_or(0);
        let y: i32      = get(idx_y).parse().unwrap_or(0);
        let name        = get(idx_name).to_string();

        planets.push(UniversePlanet { number, x, y, name });
    }

    let dump = UniverseDump { planets };
    let json = serde_json::to_string_pretty(&dump).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });
    println!("{json}");
}
