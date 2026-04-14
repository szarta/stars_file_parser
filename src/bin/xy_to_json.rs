// xy_to_json — read a Stars! .xy universe file and emit universe.json on stdout.
//
// Usage:  xy_to_json <file.xy>
//
// Output is a single JSON object with map size, planet count, intended player
// count, and difficulty tier.  Note that easy and standard difficulty share an
// identical .xy signature and are reported as "easy|standard".

use std::{env, path::Path, process};

use stars_file_parser::{
    records::{first_payload_of_type, parse_file},
    universe::universe_from_type7_payload,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: xy_to_json <file.xy>");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    });

    let records = parse_file(&bytes).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    let payload = first_payload_of_type(&records, 7).unwrap_or_else(|| {
        eprintln!("no type-7 universe record found in {}", path.display());
        process::exit(1);
    });

    let params = universe_from_type7_payload(payload).unwrap_or_else(|e| {
        eprintln!("decode error: {e}");
        process::exit(1);
    });

    let json = serde_json::to_string_pretty(&params).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });

    println!("{json}");
}
