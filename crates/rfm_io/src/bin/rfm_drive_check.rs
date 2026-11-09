//! Checks a driven run: what the recorder saw against what the adapter
//! injected.
//!
//! usage: rfm_drive_check <movement.csv> <drive_log.csv> [actions.csv] [out.csv]
//!
//! Both files come from one run with `IW4L_RFM_RECORD` and `IW4L_RFM_DRIVE`
//! (+ `IW4L_RFM_DRIVE_LOG`); rows join on `frame`. Per driven command:
//! * the command the predictor ran (recorder) equals the one the adapter
//!   built (log), and its `msec` equals the one the adapter planned with;
//! * with `actions.csv`: the log's action equals the file's row at `cursor`;
//! * intended velocity: recorded against the target, in the world frame.
//!   A miss is split into "no command reaches the target" (no `i8` command,
//!   searched exhaustively under the recorded state, misses by less) and "the
//!   adapter's guess of the state after the command was off" (a closer
//!   command existed under the recorded state), the rest being the inverse's
//!   own search falling short of the exhaustive one. While sprint is desired only commands that keep the
//!   sprint count, as in the adapter;
//! * rates: recorded against the commanded rate, exact in angle units, and
//!   against the action's clamped rate within one angle unit;
//! * sprint, stance: the state after the command against the desired one;
//! * buttons: the action's buttons in the command.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};

use movement_iw4::SHORT2ANGLE;
use playerstate_iw4::pm_flags;
use rfm_io::action::{Basis, MoveFrame, MoveScales};
use rfm_io::drive::{ACTION_BUTTONS, SPRINT_FORWARD_MIN, Stance};
use rfm_io::table::{Header, Row};

fn read(path: &str) -> std::io::Result<(Header, Vec<Row>)> {
    let mut lines = BufReader::new(std::fs::File::open(path)?).lines();
    let Some(first) = lines.next().transpose()? else {
        return Err(std::io::Error::other(format!("{path} is empty")));
    };
    let header = Header::parse(&first);
    let mut rows = Vec::new();
    for line in lines {
        rows.push(Row::parse(&line?));
    }
    Ok((header, rows))
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn percentile(values: &mut [f32], p: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * p).round() as usize]
}

/// The smallest miss of any `i8` command with `forwardmove >= forward_min`
/// from `start`, by exhaustive search.
fn closest_reach(frame: &MoveFrame<'_>, start: [f32; 2], target: [f32; 2], forward_min: i8) -> f32 {
    let mut best = f32::INFINITY;
    for f in forward_min..=127i8 {
        for r in -127..=127i8 {
            best = best.min(dist(frame.advance(start, f, r).velocity, target));
        }
    }
    best
}

fn pct(n: u64, of: u64) -> f64 {
    100.0 * n as f64 / of.max(1) as f64
}

#[derive(Default)]
struct Counts {
    driven: u64,
    not_recorded: u64,
    cmd_equal: u64,
    as_built: u64,
    msec_equal: u64,
    action_equal: u64,
    action_checked: u64,
    world_exact: u64,
    reachable: u64,
    reachable_exact: u64,
    frame_exact: u64,
    unreachable: u64,
    guess_off: u64,
    search: u64,
    inverse_short: u64,
    guess_off_sprint: u64,
    guess_off_stance: u64,
    guess_off_other: u64,
    rate_equal: u64,
    rate_teleport: u64,
    rate_within_unit: u64,
    rate_pitch_limited: u64,
    rate_clamped: u64,
    sprint_agree: u64,
    sprint_desired: u64,
    sprint_desired_agree: u64,
    stance_agree: u64,
    stance_not_stand: u64,
    stance_not_stand_agree: u64,
    buttons_equal: u64,
}

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: rfm_drive_check <movement.csv> <drive_log.csv> [actions.csv] [out.csv]");
        std::process::exit(2);
    }
    let (mh, movement) = read(&args[1])?;
    let (lh, log) = read(&args[2])?;
    let actions = match args.get(3).filter(|a| !a.is_empty()) {
        Some(path) => Some(read(path)?),
        None => None,
    };
    let mut out = match args.get(4) {
        Some(path) => {
            let mut out = BufWriter::new(std::fs::File::create(path)?);
            writeln!(
                out,
                "frame,msec,target_fwd,target_right,rec_fwd,rec_right,err,best_err,class,\
pitch_rate,yaw_rate,rec_pitch_rate,rec_yaw_rate,sprint,sprinting,stance,stance_rec"
            )?;
            Some(out)
        }
        None => None,
    };

    let by_frame: HashMap<u64, &Row> = log
        .iter()
        .filter_map(|row| Some((row.i64(&lh, "frame")? as u64, row)))
        .collect();

    let mut c = Counts::default();
    let mut errors = Vec::new();
    let mut misses = Vec::new();
    let mut gaps = Vec::new();
    let mut previous: Option<&Row> = None;
    for row in &movement {
        if row.i64(&mh, "cmd_new") != Some(1) || row.i64(&mh, "ps_valid") != Some(1) {
            continue;
        }
        let prev = previous.replace(row);
        let frame = row.i64(&mh, "frame").unwrap_or(0) as u64;
        let Some(logged) = by_frame.get(&frame) else {
            continue;
        };
        let Some(prev) = prev else {
            c.not_recorded += 1;
            continue;
        };
        c.driven += 1;
        let li = |name: &str| logged.i64(&lh, name).unwrap_or(0);
        let lf = |name: &str| logged.f32(&lh, name).unwrap_or(0.0);
        let mi = |name: &str| row.i64(&mh, name).unwrap_or(0);
        let mf = |name: &str| row.f32(&mh, name).unwrap_or(0.0);

        let same_cmd = [
            ("buttons", "buttons"),
            ("forwardmove", "forwardmove"),
            ("rightmove", "rightmove"),
            ("cmd_pitch", "cmd_pitch"),
            ("cmd_yaw", "cmd_yaw"),
            ("server_time_ms", "server_time_ms"),
        ]
        .iter()
        .all(|(m, l)| mi(m) == li(l));
        c.cmd_equal += u64::from(same_cmd);
        c.as_built += u64::from(li("as_built") == 1);
        let msec = mi("cmd_msec") as i32;
        c.msec_equal +=
            u64::from(i64::from(msec) == li("msec_built") && li("msec_run") == li("msec_built"));

        if let Some((ah, arows)) = actions.as_ref()
            && let Some(arow) = arows.get(li("cursor") as usize)
        {
            c.action_checked += 1;
            let af = |name: &str| arow.f32(ah, name).unwrap_or(0.0);
            let mut held = 0;
            for (name, bit) in rfm_io::action_file::BUTTON_COLUMNS {
                if arow.i64(ah, name).unwrap_or(0) != 0 {
                    held |= bit;
                }
            }
            let equal = af("fwd") == lf("target_fwd")
                && af("right") == lf("target_right")
                && arow.i64(ah, "sprint").unwrap_or(0) == li("sprint")
                && arow.get(ah, "stance") == logged.get(&lh, "stance")
                && i64::from(held) == li("action_buttons");
            c.action_equal += u64::from(equal);
        }

        let ps = row.player_state(&mh);
        let target_local = [lf("target_fwd"), lf("target_right")];
        let target = Basis::from_yaw(ps.viewangles[1]).unproject(target_local);
        let recorded = [mf("intended_vx"), mf("intended_vy")];
        let recorded_local = [mf("intended_fwd"), mf("intended_right")];
        let err = dist(recorded, target);
        errors.push(err);
        c.world_exact += u64::from(recorded == target);
        c.frame_exact += u64::from(recorded_local == target_local);

        let start = [
            prev.f32(&mh, "intended_vx").unwrap_or(0.0),
            prev.f32(&mh, "intended_vy").unwrap_or(0.0),
        ];
        let frame_after = MoveFrame {
            ps: &ps,
            server_time: mi("server_time_ms") as i32,
            msec,
            scales: MoveScales {
                weapon_move: mf("weapon_move_scale"),
                weapon_ads_move: mf("weapon_ads_move_scale"),
                shellshock_affects_movement: true,
            },
        };
        let forward_min = if li("sprint") == 1 {
            SPRINT_FORWARD_MIN
        } else {
            -127
        };
        let best_err = closest_reach(&frame_after, start, target, forward_min);
        let previous_move = (
            prev.i64(&mh, "forwardmove").unwrap_or(0) as i8,
            prev.i64(&mh, "rightmove").unwrap_or(0) as i8,
        );
        let chosen = frame_after.command_for_bounded(start, target, previous_move, forward_min);
        let inverse_err = dist(
            frame_after.advance(start, chosen.0, chosen.1).velocity,
            target,
        );
        let near = |a: f32, b: f32| a <= b * (1.0 + 1e-5) + 1e-5;
        if !near(inverse_err, best_err) {
            c.inverse_short += 1;
            gaps.push(inverse_err - best_err);
        }
        if best_err == 0.0 {
            c.reachable += 1;
            c.reachable_exact += u64::from(recorded == target);
        }
        let class = if recorded == target {
            "exact"
        } else if near(err, best_err) {
            c.unreachable += 1;
            misses.push(err);
            "unreachable"
        } else if near(err, inverse_err) {
            c.search += 1;
            "search"
        } else {
            c.guess_off += 1;
            let before = prev.i64(&mh, "pm_flags").unwrap_or(0) as u32;
            let changed = before ^ ps.pm_flags;
            let stance_changed = changed & (pm_flags::PRONE | pm_flags::CROUCH) != 0
                || prev.i64(&mh, "view_height_target") != row.i64(&mh, "view_height_target")
                || prev.i64(&mh, "view_height_lerp_time") != row.i64(&mh, "view_height_lerp_time");
            if changed & pm_flags::SPRINTING != 0 {
                c.guess_off_sprint += 1;
                "guess_off_sprint"
            } else if stance_changed {
                c.guess_off_stance += 1;
                "guess_off_stance"
            } else {
                c.guess_off_other += 1;
                "guess_off_other"
            }
        };

        let teleported = prev.f32(&mh, "delta_pitch") != row.f32(&mh, "delta_pitch")
            || prev.f32(&mh, "delta_yaw") != row.f32(&mh, "delta_yaw");
        let rec_rate = [mf("pitch_rate_dps"), mf("yaw_rate_dps")];
        let cmd_rate = [lf("pitch_rate_cmd"), lf("yaw_rate_cmd")];
        let action_rate = [lf("pitch_rate"), lf("yaw_rate")];
        if teleported {
            c.rate_teleport += 1;
        } else if rec_rate == cmd_rate {
            c.rate_equal += 1;
        }
        let unit = SHORT2ANGLE / (msec.max(1) as f32 * 0.001);
        let within = |axis: usize| (cmd_rate[axis] - action_rate[axis]).abs() <= unit * 0.5 + 1e-3;
        if within(0) && within(1) {
            c.rate_within_unit += 1;
        } else if within(1) && mf("view_pitch").abs() >= 84.99 {
            c.rate_pitch_limited += 1;
        }
        if action_rate[0].abs() >= rfm_io::action::PITCH_RATE_LIMIT_DPS
            || action_rate[1].abs() >= rfm_io::action::YAW_RATE_LIMIT_DPS
        {
            c.rate_clamped += 1;
        }

        let sprint = li("sprint") == 1;
        let sprinting = ps.pm_flags & pm_flags::SPRINTING != 0;
        c.sprint_agree += u64::from(sprint == sprinting);
        c.sprint_desired += u64::from(sprint);
        c.sprint_desired_agree += u64::from(sprint && sprinting);
        let stance = logged.get(&lh, "stance").unwrap_or("stand");
        let stance_rec = Stance::of(&ps).name();
        c.stance_agree += u64::from(stance == stance_rec);
        if stance != "stand" {
            c.stance_not_stand += 1;
            c.stance_not_stand_agree += u64::from(stance == stance_rec);
        }
        let emitted = mi("buttons") as u32;
        c.buttons_equal += u64::from(i64::from(emitted & ACTION_BUTTONS) == li("action_buttons"));

        if let Some(out) = out.as_mut() {
            writeln!(
                out,
                "{frame},{msec},{},{},{},{},{err},{best_err},{class},{},{},{},{},{},{},{stance},{stance_rec}",
                target_local[0],
                target_local[1],
                recorded_local[0],
                recorded_local[1],
                action_rate[0],
                action_rate[1],
                rec_rate[0],
                rec_rate[1],
                u8::from(sprint),
                u8::from(sprinting),
            )?;
        }
    }
    if let Some(out) = out.as_mut() {
        out.flush()?;
    }

    let n = c.driven;
    println!(
        "driven commands seen by the recorder: {n}, plus {} without a recorded predecessor (log rows: {})",
        c.not_recorded,
        log.len()
    );
    println!(
        "command run = command built: {:.2} % by fields, {:.2} % whole (as_built)",
        pct(c.cmd_equal, n),
        pct(c.as_built, n)
    );
    println!(
        "msec planned = msec run = recorded: {:.2} %",
        pct(c.msec_equal, n)
    );
    if actions.is_some() {
        println!(
            "logged action = action file row: {:.2} % of {}",
            pct(c.action_equal, c.action_checked),
            c.action_checked
        );
    }
    println!(
        "intended velocity = target: {:.2} % bit-exact (world), {:.2} % bit-exact (yaw frame)",
        pct(c.world_exact, n),
        pct(c.frame_exact, n)
    );
    println!(
        "  where some command lands exactly on the target ({} commands): {:.2} % bit-exact",
        c.reachable,
        pct(c.reachable_exact, c.reachable)
    );
    println!(
        "  misses: no command reaches the target {} ({:.2} %); the inverse's search falls short of the exhaustive best {} ({:.2} %); state guess off {} ({:.2} %: sprint change {}, stance change {}, other {})",
        c.unreachable,
        pct(c.unreachable, n),
        c.search,
        pct(c.search, n),
        c.guess_off,
        pct(c.guess_off, n),
        c.guess_off_sprint,
        c.guess_off_stance,
        c.guess_off_other
    );
    println!(
        "  error u/s: p50 {:.4}  p90 {:.4}  p99 {:.4}  max {:.4}; unreachable misses p50 {:.4} max {:.4}",
        percentile(&mut errors, 0.5),
        percentile(&mut errors, 0.9),
        percentile(&mut errors, 0.99),
        percentile(&mut errors, 1.0),
        percentile(&mut misses, 0.5),
        percentile(&mut misses, 1.0),
    );
    println!(
        "  inverse under the recorded state vs exhaustive search: short on {} commands ({:.2} %), by p50 {:.4} max {:.4} u/s",
        c.inverse_short,
        pct(c.inverse_short, n),
        percentile(&mut gaps, 0.5),
        percentile(&mut gaps, 1.0),
    );
    println!(
        "rates: recorded = commanded {:.2} % (+{} teleports, recorded as 0); commanded within half an angle unit of the action's clamped rate {:.2} %; pitch held at the view limit {}; action rate at a limit {}",
        pct(c.rate_equal, n),
        c.rate_teleport,
        pct(c.rate_within_unit, n),
        c.rate_pitch_limited,
        c.rate_clamped
    );
    println!(
        "sprint state = desired {:.2} %; while desired {:.2} % of {}",
        pct(c.sprint_agree, n),
        pct(c.sprint_desired_agree, c.sprint_desired),
        c.sprint_desired
    );
    println!(
        "stance state = desired {:.2} %; crouch/prone desired {:.2} % of {}",
        pct(c.stance_agree, n),
        pct(c.stance_not_stand_agree, c.stance_not_stand),
        c.stance_not_stand
    );
    println!(
        "action buttons in the command: {:.2} %",
        pct(c.buttons_equal, n)
    );
    Ok(())
}
