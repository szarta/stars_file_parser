// dump_records — print all record types and lengths from a .m1 file.
use std::{env, path::Path, process};
use stars_file_parser::records::parse_file;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { eprintln!("Usage: dump_records <file.m1> [--hex]"); process::exit(1); }
    let hex_mode = args.get(2).map(|s| s == "--hex").unwrap_or(false);
    let bytes = std::fs::read(Path::new(&args[1])).unwrap();
    let records = parse_file(&bytes).unwrap();
    for (i, rec) in records.iter().enumerate() {
        if hex_mode {
            let p = &rec.payload;
            let hex: Vec<String> = p.iter().enumerate().map(|(j,b)| format!("b{j}={b:02x}")).collect();
            println!("[{i:03}] type={:2} len={:3}  {}", rec.rtype, p.len(), hex.join(" "));
        } else {
            println!("[{i:03}] type={:2} len={:3}", rec.rtype, rec.payload.len());
        }
    }
}
