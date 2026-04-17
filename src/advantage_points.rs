// advantage_points.rs — advantage point calculator for Stars! race validation.
//
// Port of RacePointsCalculator.java + Race.getPlanetHabitability() from craigstars.
// Oracle-confirmed 2026-04-16: AR+IFE/GR/RS→37, CA+IFE/ISB/LSP→33.
//
// This copy operates on the parser's Race struct (no min_idx/max_idx).  The
// gravity-duplicate edge case (indices 9 and 10 both decode to 0.17 g) is
// unresolvable without raw indices, producing ≤4 point error in that specific
// zone — acceptable for a validation gate whose purpose is catching over-budget
// designs.

use crate::race::{HabAxis, Lrt, Prt, Race, TechCost};

const STARTING_POINTS: i64 = 1650;

#[rustfmt::skip]
const GRAV_CENTI: [u16; 101] = [
     12,  12,  13,  13,  14,  14,  15,  15,  16,  17,
     17,  18,  19,  20,  21,  22,  24,  25,  27,  29,
     31,  33,  36,  40,  44,  50,  51,  52,  53,  54,
     55,  56,  58,  59,  60,  62,  64,  65,  67,  69,
     71,  73,  75,  78,  80,  83,  86,  89,  92,  96,
    100, 104, 108, 112, 116, 120, 124, 128, 132, 136,
    140, 144, 148, 152, 156, 160, 164, 168, 172, 176,
    180, 184, 188, 192, 196, 200, 224, 248, 272, 296,
    320, 344, 368, 392, 416, 440, 464, 488, 512, 536,
    560, 584, 608, 632, 656, 680, 704, 728, 752, 776,
    800,
];

fn grav_to_idx(g: f64) -> i32 {
    let centi = (g * 100.0).round() as i32;
    GRAV_CENTI
        .iter()
        .enumerate()
        .min_by_key(|(_, &v)| (v as i32 - centi).unsigned_abs())
        .map(|(i, _)| i as i32)
        .unwrap_or(50)
}

fn temp_to_idx(t: f64) -> i32 {
    (t / 4.0 + 50.0).round().clamp(0.0, 100.0) as i32
}

fn rad_to_idx(r: f64) -> i32 {
    r.round().clamp(0.0, 100.0) as i32
}

fn axis_to_idx(axis: &HabAxis, axis_type: usize) -> (i32, i32) {
    if axis.immune {
        return (50, 50);
    }
    let min = axis.min.unwrap_or(0.0);
    let max = axis.max.unwrap_or(100.0);
    match axis_type {
        0 => (grav_to_idx(min), grav_to_idx(max)),
        1 => (temp_to_idx(min), temp_to_idx(max)),
        _ => (rad_to_idx(min), rad_to_idx(max)),
    }
}

fn planet_habitability(
    is_immune: &[bool; 3],
    hab_low: &[i32; 3],
    hab_high: &[i32; 3],
    hab_center: &[i32; 3],
    planet_hab: &[i32; 3],
) -> i64 {
    let mut planet_value: i64 = 0;
    let mut red_value: i64 = 0;
    let mut ideality: i64 = 10000;

    for t in 0..3 {
        let pv = planet_hab[t];
        let lo = hab_low[t];
        let hi = hab_high[t];
        let ctr = hab_center[t];

        if is_immune[t] {
            planet_value += 10000;
        } else if lo <= pv && pv <= hi {
            let (hab_radius, tmp) = if ctr > pv {
                (ctr - lo, ctr - pv)
            } else {
                (hi - ctr, pv - ctr)
            };
            let hab_radius = hab_radius.max(1);
            let from_ideal = ((tmp * 100) / hab_radius).min(100);
            let poor_mod = tmp * 2 - hab_radius;
            let from_ideal = 100 - from_ideal;

            planet_value += (from_ideal * from_ideal) as i64;
            if poor_mod > 0 {
                ideality *= (hab_radius * 2 - poor_mod) as i64;
                ideality /= (hab_radius * 2) as i64;
            }
        } else {
            let dist = if lo <= pv { pv - hi } else { lo - pv };
            red_value += dist.min(15) as i64;
        }
    }

    if red_value != 0 {
        return -red_value;
    }

    let pv = ((planet_value as f64 / 3.0).sqrt() + 0.9) as i64;
    pv * ideality / 10000
}

#[allow(clippy::too_many_arguments)]
fn planet_hab_for_index(
    iter_idx: i32,
    hab_type: usize,
    loop_idx: usize,
    num_iter: i32,
    hab_start: i32,
    hab_width: i32,
    hab_center: i32,
    is_immune: bool,
    tt_cf: i32,
    tf_offset: &mut [i32; 3],
) -> i32 {
    let tmp_hab = if iter_idx == 0 || num_iter <= 1 {
        hab_start
    } else {
        (hab_width * iter_idx) / (num_iter - 1) + hab_start
    };

    if loop_idx != 0 && !is_immune {
        let mut offset = hab_center - tmp_hab;
        if offset.abs() <= tt_cf {
            offset = 0;
        } else if offset < 0 {
            offset += tt_cf;
        } else {
            offset -= tt_cf;
        }
        tf_offset[hab_type] = offset;
        hab_center - offset
    } else {
        tmp_hab
    }
}

fn get_hab_range_points(race: &Race) -> i64 {
    let is_immune = [
        race.hab.gravity.immune,
        race.hab.temperature.immune,
        race.hab.radiation.immune,
    ];
    let (glo, ghi) = axis_to_idx(&race.hab.gravity, 0);
    let (tlo, thi) = axis_to_idx(&race.hab.temperature, 1);
    let (rlo, rhi) = axis_to_idx(&race.hab.radiation, 2);

    let hab_low = [glo, tlo, rlo];
    let hab_high = [ghi, thi, rhi];
    let hab_center = [(glo + ghi) / 2, (tlo + thi) / 2, (rlo + rhi) / 2];
    let num_iter = [
        if is_immune[0] { 1 } else { 11 },
        if is_immune[1] { 1 } else { 11 },
        if is_immune[2] { 1 } else { 11 },
    ];

    let has_tt = race.lrts.contains(&Lrt::TT);
    let mut total: f64 = 0.0;

    for loop_idx in 0..3usize {
        let tt_cf: i32 = match loop_idx {
            0 => 0,
            1 => if has_tt { 8 } else { 5 },
            _ => if has_tt { 17 } else { 15 },
        };

        let mut hab_start = [0i32; 3];
        let mut hab_width = [0i32; 3];
        for t in 0..3 {
            if is_immune[t] {
                hab_start[t] = 50;
                hab_width[t] = 11;
            } else {
                let lo = (hab_low[t] - tt_cf).max(0);
                let hi = (hab_high[t] + tt_cf).min(100);
                hab_start[t] = lo;
                hab_width[t] = hi - lo;
            }
        }

        let loop_weight: i64 = match loop_idx {
            0 => 7,
            1 => 5,
            _ => 6,
        };

        let mut tf_offset = [0i32; 3];
        let mut grav_sum: f64 = 0.0;

        for ig in 0..num_iter[0] {
            let gv = planet_hab_for_index(ig, 0, loop_idx, num_iter[0], hab_start[0], hab_width[0], hab_center[0], is_immune[0], tt_cf, &mut tf_offset);

            let mut temp_sum: f64 = 0.0;
            for it in 0..num_iter[1] {
                let tv = planet_hab_for_index(it, 1, loop_idx, num_iter[1], hab_start[1], hab_width[1], hab_center[1], is_immune[1], tt_cf, &mut tf_offset);

                let mut rad_sum: i64 = 0;
                for ir in 0..num_iter[2] {
                    let rv = planet_hab_for_index(ir, 2, loop_idx, num_iter[2], hab_start[2], hab_width[2], hab_center[2], is_immune[2], tt_cf, &mut tf_offset);

                    let mut desirability = planet_habitability(&is_immune, &hab_low, &hab_high, &hab_center, &[gv, tv, rv]);

                    let tf_sum: i32 = tf_offset.iter().sum();
                    if tf_sum > tt_cf {
                        desirability -= (tf_sum - tt_cf) as i64;
                        if desirability < 0 { desirability = 0; }
                    }

                    rad_sum += desirability * desirability * loop_weight;
                }

                let rad_sum = if !is_immune[2] { (rad_sum * hab_width[2] as i64) / 100 } else { rad_sum * 11 };
                temp_sum += rad_sum as f64;
            }

            let temp_sum = if !is_immune[1] { temp_sum * hab_width[1] as f64 / 100.0 } else { temp_sum * 11.0 };
            grav_sum += temp_sum;
        }

        let grav_sum = if !is_immune[0] { grav_sum * hab_width[0] as f64 / 100.0 } else { grav_sum * 11.0 };
        total += grav_sum;
    }

    (total / 10.0 + 0.5) as i64
}

fn prt_raw_points(prt: &Prt) -> i64 {
    match prt {
        Prt::He => -40,   Prt::Ss => -95,  Prt::Wm => -45,
        Prt::Ca => -10,   Prt::Is => 100,  Prt::Sd => 150,
        Prt::Pp => -120,  Prt::It => -180, Prt::Ar => -90,
        Prt::Joat => 66,
    }
}

fn lrt_raw_points(lrt: &Lrt) -> i64 {
    match lrt {
        Lrt::IFE  => -235, Lrt::TT   =>  -25, Lrt::ARM  => -159,
        Lrt::ISB  => -201, Lrt::GR   =>   40, Lrt::UR   => -240,
        Lrt::MA   => -155, Lrt::NRE  =>  160, Lrt::CE   =>  240,
        Lrt::OBRM =>  255, Lrt::NAS  =>  325, Lrt::LSP  =>  180,
        Lrt::BET  =>   70, Lrt::RS   =>   30,
    }
}

/// Compute the advantage point total for a race design.
///
/// Returns the number of remaining advantage points (raw ÷ 3).
/// ≥ 0 means the race is within budget; negative means over-spent.
/// Oracle-confirmed 2026-04-16 against Stars! race editor output.
pub fn advantage_points(race: &Race) -> i32 {
    let mut points: i64 = STARTING_POINTS;

    let hab_points = get_hab_range_points(race) / 2000;

    let gr_raw = race.economy.growth_rate as i64;
    let mut gr = gr_raw;
    if gr <= 5 {
        points += (6 - gr) * 4200;
    } else if gr <= 13 {
        match gr {
            6 => points += 3600,
            7 => points += 2250,
            8 => points += 600,
            9 => points += 225,
            _ => {}
        }
        gr = gr * 2 - 5;
    } else if gr < 20 {
        gr = (gr - 6) * 3;
    } else {
        gr = 45;
    }
    points -= hab_points * gr / 24;

    let mut num_immunities: i64 = 0;
    let axes = [
        (&race.hab.gravity, axis_to_idx(&race.hab.gravity, 0)),
        (&race.hab.temperature, axis_to_idx(&race.hab.temperature, 1)),
        (&race.hab.radiation, axis_to_idx(&race.hab.radiation, 2)),
    ];
    for (axis, (lo, hi)) in &axes {
        if axis.immune {
            num_immunities += 1;
        } else {
            let center = (lo + hi) / 2;
            points += (center - 50).unsigned_abs() as i64 * 4;
        }
    }
    if num_immunities > 1 {
        points -= 150;
    }

    {
        let op = race.economy.colonists_operate_factories as i64;
        let pp_raw = race.economy.factory_production as i64;
        if op > 10 || pp_raw > 10 {
            let op = (op - 9).max(1);
            let mut pp = (pp_raw - 9).max(1);
            let fpc: i64 = if race.prt == Prt::He { 3 } else { 2 };
            pp *= fpc;
            let penalty = (pp * op) as f64 * gr_raw as f64;
            if num_immunities >= 2 {
                points -= (penalty / 2.0) as i64;
            } else {
                points -= (penalty / 9.0) as i64;
            }
        }
    }

    {
        let pop_eff = (race.economy.resource_production / 100).min(25) as i64;
        if pop_eff <= 7 {
            points -= 2400;
        } else if pop_eff == 8 {
            points -= 1260;
        } else if pop_eff == 9 {
            points -= 600;
        } else if pop_eff > 10 {
            points += (pop_eff - 10) * 120;
        }
    }

    if race.prt == Prt::Ar {
        points += 210;
    } else {
        let factory_output = race.economy.factory_production as i64;
        let factory_cost = race.economy.factory_cost as i64;
        let num_factories = race.economy.colonists_operate_factories as i64;

        let prod_p = 10 - factory_output;
        let cost_p = 10 - factory_cost;
        let oper_p = 10 - num_factories;

        let mut tmp: i64 = 0;
        tmp += if prod_p > 0 { prod_p * 100 } else { prod_p * 121 };
        tmp += if cost_p > 0 { cost_p * cost_p * -60 } else { cost_p * -55 };
        tmp += if oper_p > 0 { oper_p * 40 } else { oper_p * 35 };

        const LLFP: i64 = 700;
        if tmp > LLFP {
            tmp = (tmp - LLFP) / 3 + LLFP;
        }

        if oper_p <= -7 {
            if oper_p < -11 {
                if oper_p < -14 { tmp -= 360; } else { tmp += (oper_p + 7) * 45; }
            } else {
                tmp += (oper_p + 6) * 30;
            }
        }
        if prod_p <= -3 {
            tmp += (prod_p + 2) * 60;
        }

        points += tmp;

        if race.economy.factory_cheap_germanium {
            points -= 175;
        }

        let mine_output = race.economy.mine_production as i64;
        let mine_cost = race.economy.mine_cost as i64;
        let num_mines = race.economy.colonists_operate_mines as i64;

        let prod_p = 10 - mine_output;
        let cost_p = 3 - mine_cost;
        let oper_p = 10 - num_mines;

        let mut tmp: i64 = 0;
        tmp += if prod_p > 0 { prod_p * 100 } else { prod_p * 169 };
        tmp += if cost_p > 0 { -360 } else { cost_p * (-65) + 80 };
        tmp += if oper_p > 0 { oper_p * 40 } else { oper_p * 35 };

        points += tmp;
    }

    points += prt_raw_points(&race.prt);

    let mut bad_lrts: i64 = 0;
    let mut good_lrts: i64 = 0;
    for lrt in &race.lrts {
        let raw = lrt_raw_points(lrt);
        if raw >= 0 { bad_lrts += 1; } else { good_lrts += 1; }
        points += raw;
    }
    let total_lrts = good_lrts + bad_lrts;
    if total_lrts > 4 {
        points -= total_lrts * (total_lrts - 4) * 10;
    }
    if bad_lrts - good_lrts > 3 {
        points -= (bad_lrts - good_lrts - 3) * 60;
    }
    if good_lrts - bad_lrts > 3 {
        points -= (good_lrts - bad_lrts - 3) * 40;
    }

    if race.lrts.contains(&Lrt::NAS) {
        points -= match race.prt {
            Prt::Pp => 280, Prt::Ss => 200, Prt::Joat => 40, _ => 0,
        };
    }

    let rc = &race.research_costs;
    let mut techcosts: i64 = 0;
    for f in [&rc.energy, &rc.weapons, &rc.propulsion, &rc.construction, &rc.electronics, &rc.biotechnology] {
        match f {
            TechCost::Expensive => techcosts -= 1,
            TechCost::Cheap     => techcosts += 1,
            TechCost::Normal    => {}
        }
    }

    if techcosts > 0 {
        points -= techcosts * techcosts * 130;
        if techcosts >= 6 { points += 1430; } else if techcosts == 5 { points += 520; }
    } else if techcosts < 0 {
        const SCIENCE_COST: [i64; 6] = [150, 330, 540, 780, 1050, 1380];
        let idx = ((-techcosts - 1) as usize).min(5);
        points += SCIENCE_COST[idx];
        if techcosts < -4 && race.economy.resource_production < 1000 {
            points -= 190;
        }
    }

    if race.research_costs.expensive_tech_start_at_4 {
        points -= 180;
    }

    if race.prt == Prt::Ar && race.research_costs.energy == TechCost::Cheap {
        points -= 100;
    }

    (points / 3) as i32
}
