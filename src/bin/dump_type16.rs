// dump_type16 — hex-dump all type-16 FleetBlock records from a .m1 file.
// Usage: dump_type16 <file.m1>
use std::{env, path::Path, process};
use stars_file_parser::records::parse_file;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: dump_type16 <file.m1>");
        process::exit(1);
    }
    let bytes = std::fs::read(Path::new(&args[1])).unwrap();
    let records = parse_file(&bytes).unwrap();
    let mut idx = 0usize;
    for rec in &records {
        if rec.rtype != 16 { continue; }
        let p = &rec.payload;
        let fleet_idx = u16::from_le_bytes([p[0], p[1]]);
        print!("fleet={fleet_idx} len={} bytes: ", p.len());
        for (i, b) in p.iter().enumerate() {
            print!("b{i}={b:02X}({b}) ");
        }
        println!();
        idx += 1;
    }
    let _ = idx;
}
