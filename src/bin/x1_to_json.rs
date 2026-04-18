// x1_to_json — decode a Stars! .xN player orders file and emit JSON on stdout.
//
// Usage:  x1_to_json <file.x1>
//
// Output is a JSON object:
//   "player_idx"  — 0-based player index (low 5 bits of type-8 b12-13 word)
//   "orders"      — list of decoded order objects, each with a "type" field
//
// The .x1 file uses the same container/cipher format as .m1 files.  The host
// applies orders in file order; each Save in the UI appends new records.
//
// Confirmed order types (from reverse-engineering/PLAN.md R0.7, 2026-04-17):
//
//   type-1  manual_cargo_transfer  — waypoint-0 manual cargo load/unload
//   type-3  waypoint_delete        — delete a waypoint from a fleet's route
//   type-4  waypoint_add           — add a new waypoint (always paired with type-5)
//   type-5  waypoint_change_task   — set/change waypoint task (authoritative for task)
//   type-10 waypoint_repeat        — enable/disable repeat orders for a fleet
//   type-23 move_ships             — ship selection for a fleet split (paired with type-24)
//   type-24 fleet_split            — initiate a fleet split (paired with type-23)
//   type-29 production_queue_change — replace a planet's production queue
//   type-30 battle_plan            — create or update a battle plan
//   type-34 research_change        — change research allocation
//   type-37 fleets_merge           — merge one or more fleets into a base fleet
//   type-42 set_fleet_battle_plan  — assign a battle plan to a fleet
//   type-44 rename_fleet           — rename a fleet
//
// Transport payload encoding (type-4/5, confirmed 2026-04-17):
//   2 bytes per resource in fixed order: iron, boran, germ, col, fuel.
//   Each pair: lo = amount & 0xFF; hi = (action << 4) | ((amount >> 8) & 0xF)
//   Trailing (0x00, 0x00) pairs omitted → record length varies.
//   Actions: 0=None, 1=LoadAll, 2=UnloadAll, 3=LoadExact, 4=UnloadExact, 5=FillUpTo.
//   Fuel amount is a percentage (0–100); mineral/col amounts in kT.

use std::{env, path::Path, process};

use serde::Serialize;
use stars_file_parser::records::parse_file;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Read a 9-bit fleet number from payload bytes 0-1.
/// Encoding: fleet_num = b0 | ((b1 & 1) << 8).  Range 0–511.
fn fleet_num_9bit(p: &[u8]) -> u16 {
    if p.len() < 2 { return 0; }
    (p[0] as u16) | (((p[1] & 1) as u16) << 8)
}

fn read_u16_le(p: &[u8], off: usize) -> u16 {
    if off + 2 <= p.len() { u16::from_le_bytes([p[off], p[off + 1]]) } else { 0 }
}

/// Decode a Stars! nibble-encoded name.
///
/// Format: data[0] = byte count; data[1..1+count] = packed nibbles.
/// Nibble 0x0-0xA → ONE_NIBBLE[n]; 0xB-0xE → prefix + index; 0xF → raw ASCII.
fn decode_stars_name(data: &[u8]) -> String {
    if data.is_empty() { return String::new(); }
    let name_len = data[0] as usize;
    if data.len() < 1 + name_len { return String::new(); }
    let encoded = &data[1..1 + name_len];

    const ONE: [char; 11] = [' ','a','e','h','i','l','n','o','r','s','t'];
    const BT:  [char; 16] = ['A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P'];
    const CT:  [char; 16] = ['Q','R','S','T','U','V','W','X','Y','Z','0','1','2','3','4','5'];
    const DT:  [char; 16] = ['6','7','8','9','b','c','d','f','g','j','k','m','p','q','u','v'];
    const ET:  [char; 16] = ['w','x','y','z','+','-',',','!','.','?',':',';','\'','*','%','$'];

    let nibs: Vec<u8> = encoded.iter().flat_map(|&b| [(b >> 4) & 0xF, b & 0xF]).collect();
    let mut result = String::new();
    let mut i = 0;
    let limit = name_len * 2;
    while i < limit && i < nibs.len() {
        let n = nibs[i];
        match n {
            0..=10 => { result.push(ONE[n as usize]); i += 1; }
            11 => { i += 1; if i < nibs.len() { result.push(BT[nibs[i] as usize]); i += 1; } }
            12 => { i += 1; if i < nibs.len() { result.push(CT[nibs[i] as usize]); i += 1; } }
            13 => { i += 1; if i < nibs.len() { result.push(DT[nibs[i] as usize]); i += 1; } }
            14 => { i += 1; if i < nibs.len() { result.push(ET[nibs[i] as usize]); i += 1; } }
            15 => {
                i += 1;
                if i + 1 < nibs.len() {
                    let lo = nibs[i] as u32; i += 1;
                    let hi = nibs[i] as u32; i += 1;
                    if let Some(c) = char::from_u32((lo << 4) | hi) { result.push(c); }
                }
            }
            _ => { i += 1; }
        }
    }
    result
}

// ── Transport payload ─────────────────────────────────────────────────────────

/// Per-resource cargo action in a Transport waypoint task.
///
/// Each resource occupies 2 bytes in the payload (lo, hi):
///   amount = lo | ((hi & 0x0F) << 8)   — kT (fuel = percentage 0–100)
///   action = hi >> 4
///
/// Actions: 0=None, 1=LoadAll, 2=UnloadAll, 3=LoadExact, 4=UnloadExact, 5=FillUpTo
/// Confirmed in x1 (2026-04-17): 1=LoadAll ✓, 2=UnloadAll ✓, 3=LoadExact ✓, 5=FillUpTo ✓.
#[derive(Debug, Serialize)]
struct ResourceOp {
    amount: u16,
    action: u8,
}

fn decode_resource_op(lo: u8, hi: u8) -> ResourceOp {
    ResourceOp {
        amount: (lo as u16) | (((hi & 0x0F) as u16) << 8),
        action: hi >> 4,
    }
}

/// Read the resource pair at position `idx` (0=iron, 1=boran, 2=germ, 3=col, 4=fuel)
/// from the transport payload slice.  Returns None-action op if out of range.
fn transport_op(payload: &[u8], idx: usize) -> ResourceOp {
    let off = idx * 2;
    if off + 2 <= payload.len() {
        decode_resource_op(payload[off], payload[off + 1])
    } else {
        ResourceOp { amount: 0, action: 0 }
    }
}

#[derive(Debug, Serialize)]
struct TransportPayload {
    ironium:   ResourceOp,
    boranium:  ResourceOp,
    germanium: ResourceOp,
    colonists: ResourceOp,
    fuel:      ResourceOp,
}

fn decode_transport(p: &[u8], start: usize) -> TransportPayload {
    let payload = if start < p.len() { &p[start..] } else { &[] };
    TransportPayload {
        ironium:   transport_op(payload, 0),
        boranium:  transport_op(payload, 1),
        germanium: transport_op(payload, 2),
        colonists: transport_op(payload, 3),
        fuel:      transport_op(payload, 4),
    }
}

// ── Order types ───────────────────────────────────────────────────────────────

/// One decoded production queue item (type-29).
///
/// Encoding: two consecutive LE uint16 words:
///   word0 = (item_id << 10) | count          — count=items to build, item_id=what to build
///   word1 = (complete_percent << 4) | item_type
///
/// item_type: 2=standard item, 4=custom ship design
/// Standard item_id: 7=Factory, 8=Mine, 9=Defenses, others TBD.
/// Custom design item_id: 0-based design slot index.
#[derive(Debug, Serialize)]
struct QueueItem {
    item_id:          u16,
    count:            u16,
    complete_percent: u16,
    item_type:        u8,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Order {
    /// Type-1 ManualSmallLoadUnloadTaskBlock.
    ///
    /// Waypoint-0 cargo transfer (immediate, at current planet).
    /// One record per transfer event; variable length based on how many
    /// resources are set.  action byte b4 = 0x12 in all load experiments.
    ///
    /// resource_mask bits: 0=ironium, 1=boranium, 2=germanium, 3=colonists.
    /// amounts: one entry per set bit in ascending bit order (iron, boran, germ, col).
    ManualCargoTransfer {
        fleet_num:     u16,
        planet_idx:    u16,
        action_byte:   u8,
        resource_mask: u8,
        /// Amounts in kT, in ascending resource order (iron, boran, germ, col).
        amounts:       Vec<u8>,
    },

    /// Type-3 WaypointDeleteBlock.
    WaypointDelete {
        fleet_num:   u16,
        waypoint_nr: u8,
    },

    /// Type-4 WaypointAddBlock.
    ///
    /// Establishes destination; always paired with a type-5 that sets the task.
    /// Type-5 is authoritative for the task — use it for task decoding.
    WaypointAdd {
        fleet_num:   u16,
        waypoint_nr: u8,
        dest_x:      u16,
        dest_y:      u16,
        /// Full LE uint16 target index (planet or fleet); NOT 9-bit.
        target:      u16,
        warp_speed:  u8,
        task_type:   u8,
        target_type: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        transport:   Option<TransportPayload>,
    },

    /// Type-5 WaypointChangeTaskBlock (authoritative for task).
    ///
    /// Identical byte layout to type-4.  task_type codes:
    /// 0=None, 1=Transport, 2=Colonize, 3=RemoteMining, 4=MergeWithFleet,
    /// 5=ScrapFleet, 6=LayMineField, 7=Patrol, 8=Route, 9=TransferFleet.
    WaypointChangeTask {
        fleet_num:   u16,
        waypoint_nr: u8,
        dest_x:      u16,
        dest_y:      u16,
        target:      u16,
        warp_speed:  u8,
        task_type:   u8,
        target_type: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        transport:   Option<TransportPayload>,
    },

    /// Type-10 WaypointRepeatOrdersBlock.
    WaypointRepeat {
        fleet_num: u16,
        /// true = repeat orders enabled; false = disabled (1=on, 0=off).
        repeat:    bool,
    },

    /// Type-23 MoveShipsBlock (ship selection for fleet split).
    ///
    /// Always paired with a type-24 FleetSplit record.
    /// design_mask: bit k set = move ALL ships of design slot k to dst_fleet.
    /// Confirmed: bit2=Santa Maria ✓, bit3=Teamster ✓.
    /// b2–b4 and b6 are partially unknown (constant within same fleet config).
    MoveShips {
        src_fleet:   u16,
        design_mask: u8,
        /// 0xFFFF = create a new fleet.
        dst_fleet:   u16,
    },

    /// Type-24 FleetSplitBlock.
    ///
    /// Signals the fleet being split; always paired with type-23.
    FleetSplit {
        fleet_num: u16,
    },

    /// Type-29 ProductionQueueChangeBlock.
    ///
    /// Replaces the entire production queue for a planet.
    ProductionQueueChange {
        planet_idx: u16,
        items:      Vec<QueueItem>,
    },

    /// Type-30 BattlePlanBlock.
    ///
    /// Creates or updates a battle plan.  plan_index = plan_id / 16.
    /// Built-in slots 0–4 (Default/KillSB/MaxDef/Sniper/Chicken); 5+ = custom.
    /// Tactic: 0=Disengage, 1=DisIfChall, 3=MaxNetDmg, 4=MaxDmgRatio.
    /// Targets: 0=None, 1=Any, 2=Starbase, 3=Armed, 4=Bombers, 5=Unarmed,
    ///          6=FuelTransports, 7=Freighters.
    BattlePlan {
        plan_index:       u8,
        tactic:           u8,
        primary_target:   u8,
        secondary_target: u8,
        attack_who:       u8,
        name:             String,
    },

    /// Type-34 ResearchChangeBlock.
    ///
    /// Fields: 0=Energy, 1=Weapons, 2=Propulsion, 3=Construction,
    ///         4=Electronics, 5=Biology, 6=SameField (continue current).
    ResearchChange {
        research_percent: u8,
        current_field:    u8,
        next_field:       u8,
    },

    /// Type-37 FleetsMergeBlock.
    ///
    /// Merges one or more fleets into `fleet_num`.
    FleetsMerge {
        fleet_num:     u16,
        merge_targets: Vec<u16>,
    },

    /// Type-42 SetFleetBattlePlanBlock.
    ///
    /// plan_index: 0=Default, 1=KillSB, 2=MaxDef, 3=Sniper, 4=Chicken, 5+=custom.
    SetFleetBattlePlan {
        fleet_num:  u16,
        plan_index: u16,
    },

    /// Type-44 RenameFleetBlock.
    RenameFleet {
        fleet_num: u16,
        name:      String,
    },

    /// Unknown or not-yet-decoded record type.
    Unknown {
        record_type: u16,
        payload_hex: String,
    },
}

// ── Decoders ──────────────────────────────────────────────────────────────────

fn decode_type1(p: &[u8]) -> Order {
    let fleet_num   = fleet_num_9bit(p);
    let planet_idx  = read_u16_le(p, 2);
    let action_byte = if p.len() > 4 { p[4] } else { 0 };
    let mask        = if p.len() > 5 { p[5] } else { 0 };

    let mut amounts = Vec::new();
    let mut off = 6usize;
    for bit in 0..4u8 {
        if (mask >> bit) & 1 == 1 {
            amounts.push(if off < p.len() { p[off] } else { 0 });
            off += 1;
        }
    }

    Order::ManualCargoTransfer { fleet_num, planet_idx, action_byte, resource_mask: mask, amounts }
}

fn decode_type3(p: &[u8]) -> Order {
    Order::WaypointDelete {
        fleet_num:   fleet_num_9bit(p),
        waypoint_nr: if p.len() > 2 { p[2] } else { 0 },
    }
}

fn decode_waypoint_record(p: &[u8]) -> (u16, u8, u16, u16, u16, u8, u8, u8, Option<TransportPayload>) {
    let fleet_num   = fleet_num_9bit(p);
    let waypoint_nr = if p.len() > 2 { p[2] } else { 0 };
    let dest_x      = read_u16_le(p, 4);
    let dest_y      = read_u16_le(p, 6);
    let target      = read_u16_le(p, 8);
    let b10         = if p.len() > 10 { p[10] } else { 0 };
    let b11         = if p.len() > 11 { p[11] } else { 0 };
    let warp_speed  = b10 >> 4;
    let task_type   = b10 & 0xF;
    let target_type = b11 & 0xF;
    let transport   = if task_type == 1 && p.len() > 12 {
        Some(decode_transport(p, 12))
    } else {
        None
    };
    (fleet_num, waypoint_nr, dest_x, dest_y, target, warp_speed, task_type, target_type, transport)
}

fn decode_type4(p: &[u8]) -> Order {
    let (fleet_num, waypoint_nr, dest_x, dest_y, target, warp_speed, task_type, target_type, transport)
        = decode_waypoint_record(p);
    Order::WaypointAdd { fleet_num, waypoint_nr, dest_x, dest_y, target, warp_speed, task_type, target_type, transport }
}

fn decode_type5(p: &[u8]) -> Order {
    let (fleet_num, waypoint_nr, dest_x, dest_y, target, warp_speed, task_type, target_type, transport)
        = decode_waypoint_record(p);
    Order::WaypointChangeTask { fleet_num, waypoint_nr, dest_x, dest_y, target, warp_speed, task_type, target_type, transport }
}

fn decode_type10(p: &[u8]) -> Order {
    Order::WaypointRepeat {
        fleet_num: fleet_num_9bit(p),
        repeat:    read_u16_le(p, 2) != 0,
    }
}

fn decode_type23(p: &[u8]) -> Order {
    Order::MoveShips {
        src_fleet:   fleet_num_9bit(p),
        design_mask: if p.len() > 5 { p[5] } else { 0 },
        dst_fleet:   read_u16_le(p, 7),
    }
}

fn decode_type24(p: &[u8]) -> Order {
    Order::FleetSplit { fleet_num: fleet_num_9bit(p) }
}

fn decode_type29(p: &[u8]) -> Order {
    let planet_idx = read_u16_le(p, 0) & 0x7FF;
    let mut items  = Vec::new();
    let mut off    = 2usize;
    while off + 4 <= p.len() {
        let w0 = read_u16_le(p, off);
        let w1 = read_u16_le(p, off + 2);
        let count            = w0 & 0x3FF;
        let item_id          = w0 >> 10;
        let item_type        = (w1 & 0xF) as u8;
        let complete_percent = w1 >> 4;
        items.push(QueueItem { item_id, count, complete_percent, item_type });
        off += 4;
    }
    Order::ProductionQueueChange { planet_idx, items }
}

fn decode_type30(p: &[u8]) -> Order {
    Order::BattlePlan {
        plan_index:       if p.len() > 0 { p[0] / 16 } else { 0 },
        tactic:           if p.len() > 1 { p[1] } else { 0 },
        primary_target:   if p.len() > 2 { p[2] & 0xF } else { 0 },
        secondary_target: if p.len() > 2 { (p[2] >> 4) & 0xF } else { 0 },
        attack_who:       if p.len() > 3 { p[3] } else { 0 },
        name:             if p.len() > 4 { decode_stars_name(&p[4..]) } else { String::new() },
    }
}

fn decode_type34(p: &[u8]) -> Order {
    let b1 = if p.len() > 1 { p[1] } else { 0 };
    Order::ResearchChange {
        research_percent: if p.len() > 0 { p[0] } else { 0 },
        current_field:    b1 & 0xF,
        next_field:       (b1 >> 4) & 0xF,
    }
}

fn decode_type37(p: &[u8]) -> Order {
    let fleet_num     = fleet_num_9bit(p);
    let mut merge_targets = Vec::new();
    let mut off = 2usize;
    while off + 2 <= p.len() {
        merge_targets.push(read_u16_le(p, off) & 0x1FF);
        off += 2;
    }
    Order::FleetsMerge { fleet_num, merge_targets }
}

fn decode_type42(p: &[u8]) -> Order {
    Order::SetFleetBattlePlan {
        fleet_num:  fleet_num_9bit(p),
        plan_index: read_u16_le(p, 2),
    }
}

fn decode_type44(p: &[u8]) -> Order {
    Order::RenameFleet {
        fleet_num: fleet_num_9bit(p),
        name:      if p.len() > 4 { decode_stars_name(&p[4..]) } else { String::new() },
    }
}

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OrderFile {
    /// 0-based player index (low 5 bits of the seed word at type-8 b12-13).
    player_idx: u8,
    orders:     Vec<Order>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: x1_to_json <file.x1>");
        process::exit(1);
    }

    let path  = Path::new(&args[1]);
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    });

    let records = parse_file(&bytes).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    let mut player_idx: u8 = 0;
    let mut orders: Vec<Order> = Vec::new();

    for rec in &records {
        match rec.rtype {
            0 | 9 => {}  // end marker / file hash — skip
            8 => {
                // type-8 FileHeaderBlock: b12-13 = player/seed word; low 5 bits = player index
                if rec.payload.len() >= 14 {
                    let seed_word = u16::from_le_bytes([rec.payload[12], rec.payload[13]]);
                    player_idx = (seed_word & 0x1F) as u8;
                }
            }
            1  => orders.push(decode_type1(&rec.payload)),
            3  => orders.push(decode_type3(&rec.payload)),
            4  => orders.push(decode_type4(&rec.payload)),
            5  => orders.push(decode_type5(&rec.payload)),
            10 => orders.push(decode_type10(&rec.payload)),
            23 => orders.push(decode_type23(&rec.payload)),
            24 => orders.push(decode_type24(&rec.payload)),
            29 => orders.push(decode_type29(&rec.payload)),
            30 => orders.push(decode_type30(&rec.payload)),
            34 => orders.push(decode_type34(&rec.payload)),
            37 => orders.push(decode_type37(&rec.payload)),
            42 => orders.push(decode_type42(&rec.payload)),
            44 => orders.push(decode_type44(&rec.payload)),
            t  => orders.push(Order::Unknown {
                record_type: t,
                payload_hex: rec.payload.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
            }),
        }
    }

    let out  = OrderFile { player_idx, orders };
    let json = serde_json::to_string_pretty(&out).unwrap_or_else(|e| {
        eprintln!("serialization error: {e}");
        process::exit(1);
    });
    println!("{json}");
}
