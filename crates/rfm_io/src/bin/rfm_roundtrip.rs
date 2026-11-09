//! Round trip of a movement recording through the action interface.
//!
//! usage: rfm_roundtrip <recording.csv> <out.csv> [yaw_limit_dps] [pitch_limit_dps]
//!
//! For every recorded command:
//! * forward: recorded command → intended velocity, compared with the value
//!   the game recorded (checks that the offline model is the online one; only
//!   meaningful for recordings made with the same definition);
//! * inverse: that intended velocity → command → intended velocity, closed
//!   loop on the inverse's own state (what an inference adapter does), and the
//!   reconstructed command against the recorded one;
//! * rotation: recorded rates → integrated command angles, compared with the
//!   recorded angles; and how much a rate limit would cut.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, BufWriter, Write};

use playerstate_iw4::{PM_TYPE_DEAD, PM_TYPE_SPECTATOR};
use rfm_io::action::{
    Basis, MoveFrame, MoveScales, PITCH_RATE_LIMIT_DPS, RateTracker, YAW_RATE_LIMIT_DPS,
    angles_after_rate,
};
use rfm_io::table::{Header, Row};

#[derive(Default)]
struct Stats {
    commands: u64,
    moving: u64,
    forward_err: Vec<f32>,
    inverse_err: Vec<f32>,
    command_match: u64,
    command_off_by: [u64; 4],
    angle_mismatch: u64,
    angle_max_err_units: i64,
    reanchors: u64,
    zero_time: u64,
    yaw_clipped: u64,
    pitch_clipped: u64,
    yaw_cut_deg: f64,
    pitch_cut_deg: f64,
}

fn percentile(values: &mut [f32], p: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(f32::total_cmp);
    let i = ((values.len() - 1) as f32 * p).round() as usize;
    values[i]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: rfm_roundtrip <recording.csv> <out.csv> [yaw_limit_dps] [pitch_limit_dps]"
        );
        std::process::exit(2);
    }
    let yaw_limit: f32 = args
        .get(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(YAW_RATE_LIMIT_DPS);
    let pitch_limit: f32 = args
        .get(4)
        .and_then(|v| v.parse().ok())
        .unwrap_or(PITCH_RATE_LIMIT_DPS);

    let mut lines = BufReader::new(std::fs::File::open(&args[1])?).lines();
    let Some(header) = lines.next().transpose()? else {
        eprintln!("empty recording");
        std::process::exit(2);
    };
    let ix = Header::parse(&header);
    let has_full_state = ix.has("pm_time");

    let mut out = BufWriter::new(std::fs::File::create(&args[2])?);
    writeln!(
        out,
        "frame,real_s,moving,on_ground,buttons,forwardmove,rightmove,f_inv,r_inv,\
vx_rec,vy_rec,vx_fwd,vy_fwd,vx_inv,vy_inv,err_fwd,err_inv,full_speed,\
intended_fwd,intended_right,intended_fwd_norm,intended_right_norm,\
yaw_rate,pitch_rate,cmd_pitch,cmd_yaw,pitch_rt,yaw_rt,cmd_msec"
    )?;

    let mut stats = Stats::default();
    let mut v_fwd = [0.0f32; 2];
    let mut v_inv = [0.0f32; 2];
    let mut previous_cmd: (i8, i8) = (0, 0);
    let mut angles_rt: Option<[i32; 3]> = None;
    let mut rates = RateTracker::default();
    let mut last_delta: Option<[f32; 2]> = None;

    for line in lines {
        let row = Row::parse(&line?);
        if row.i64(&ix, "cmd_new") != Some(1) || row.i64(&ix, "ps_valid") != Some(1) {
            continue;
        }
        stats.commands += 1;
        let ps = row.player_state(&ix);
        let f = row.i64(&ix, "forwardmove").unwrap_or(0) as i8;
        let r = row.i64(&ix, "rightmove").unwrap_or(0) as i8;
        let msec = row.i64(&ix, "cmd_msec").unwrap_or(0) as i32;
        let server_time = row.i64(&ix, "server_time_ms").unwrap_or(0) as i32;
        let cmd_angles = [
            row.i64(&ix, "cmd_pitch").unwrap_or(0) as i32,
            row.i64(&ix, "cmd_yaw").unwrap_or(0) as i32,
            0,
        ];
        let recorded = [
            row.f32(&ix, "intended_vx").unwrap_or(0.0),
            row.f32(&ix, "intended_vy").unwrap_or(0.0),
        ];
        let scales = MoveScales {
            weapon_move: row.f32(&ix, "weapon_move_scale").unwrap_or(1.0),
            weapon_ads_move: row.f32(&ix, "weapon_ads_move_scale").unwrap_or(1.0),
            shellshock_affects_movement: true,
        };
        let moving = ps.pm_type != PM_TYPE_SPECTATOR && ps.pm_type < PM_TYPE_DEAD;
        let frame = MoveFrame {
            ps: &ps,
            server_time,
            msec,
            scales,
        };

        let (f_inv, r_inv, full_speed) = if moving {
            stats.moving += 1;
            v_fwd = frame.advance(v_fwd, f, r).velocity;
            let cmd = frame.command_for(v_inv, v_fwd, previous_cmd);
            previous_cmd = cmd;
            v_inv = frame.advance(v_inv, cmd.0, cmd.1).velocity;
            stats.forward_err.push(dist(v_fwd, recorded));
            stats.inverse_err.push(dist(v_inv, v_fwd));
            let off = (i32::from(cmd.0) - i32::from(f))
                .abs()
                .max((i32::from(cmd.1) - i32::from(r)).abs());
            if off == 0 {
                stats.command_match += 1;
            }
            stats.command_off_by[(off as usize).min(3)] += 1;
            (cmd.0, cmd.1, frame.full_speed())
        } else {
            let horizontal = [ps.velocity[0], ps.velocity[1]];
            v_fwd = horizontal;
            v_inv = horizontal;
            (0, 0, 0.0)
        };

        let delta = [ps.delta_angles[0], ps.delta_angles[1]];
        let teleported = last_delta.is_some_and(|d| d != delta);
        last_delta = Some(delta);
        let rate = rates.observe(cmd_angles, msec, teleported);
        let rt = match angles_rt {
            Some(previous) if !teleported => angles_after_rate(previous, rate, msec),
            Some(_) => {
                stats.reanchors += 1;
                cmd_angles
            }
            None => cmd_angles,
        };
        angles_rt = Some(rt);
        let err = i64::from((rt[0].wrapping_sub(cmd_angles[0]) as i16).unsigned_abs()).max(
            i64::from((rt[1].wrapping_sub(cmd_angles[1]) as i16).unsigned_abs()),
        );
        if msec <= 0 {
            stats.zero_time += 1;
        } else {
            if err != 0 {
                stats.angle_mismatch += 1;
            }
            stats.angle_max_err_units = stats.angle_max_err_units.max(err);
        }

        let seconds = f64::from(msec) * 0.001;
        if rate[1].abs() > yaw_limit {
            stats.yaw_clipped += 1;
            stats.yaw_cut_deg += f64::from(rate[1].abs() - yaw_limit) * seconds;
        }
        if rate[0].abs() > pitch_limit {
            stats.pitch_clipped += 1;
            stats.pitch_cut_deg += f64::from(rate[0].abs() - pitch_limit) * seconds;
        }

        let basis = Basis::from_yaw(ps.viewangles[1]);
        let local = basis.project(v_fwd);
        let norm = |v: f32| {
            if full_speed > 0.0 {
                v / full_speed
            } else {
                0.0
            }
        };
        let mut line = String::new();
        let _ = write!(
            line,
            "{},{},{},{},{},{f},{r},{f_inv},{r_inv},{},{},{},{},{},{},{},{},{full_speed},{},{},{},{},{},{},{},{},{},{},{msec}",
            row.get(&ix, "frame").unwrap_or(""),
            row.get(&ix, "real_s").unwrap_or(""),
            u8::from(moving),
            row.get(&ix, "on_ground").unwrap_or(""),
            row.get(&ix, "buttons").unwrap_or("0"),
            recorded[0],
            recorded[1],
            v_fwd[0],
            v_fwd[1],
            v_inv[0],
            v_inv[1],
            dist(v_fwd, recorded),
            dist(v_inv, v_fwd),
            local[0],
            local[1],
            norm(local[0]),
            norm(local[1]),
            rate[1],
            rate[0],
            cmd_angles[0],
            cmd_angles[1],
            rt[0],
            rt[1],
        );
        writeln!(out, "{line}")?;
    }
    out.flush()?;

    let moving = stats.moving.max(1) as f32;
    println!("recording has the full movement state: {has_full_state}");
    println!(
        "commands: {}  while moving: {}",
        stats.commands, stats.moving
    );
    for (name, errors) in [
        (
            "forward, offline vs recorded column",
            &mut stats.forward_err,
        ),
        (
            "inverse round trip (intended → command → intended)",
            &mut stats.inverse_err,
        ),
    ] {
        let exact = errors.iter().filter(|e| **e == 0.0).count();
        println!(
            "{name}: exact {:.2} %  p50 {:.4}  p99 {:.4}  max {:.4} u/s",
            100.0 * exact as f32 / moving,
            percentile(errors, 0.5),
            percentile(errors, 0.99),
            percentile(errors, 1.0),
        );
    }
    println!(
        "reconstructed command equals the recorded one: {:.2} %  (off by 1: {}, 2: {}, ≥3: {})",
        100.0 * stats.command_match as f32 / moving,
        stats.command_off_by[1],
        stats.command_off_by[2],
        stats.command_off_by[3],
    );
    println!(
        "rotation: integrated angles differ from recorded on {} commands, max {} units ({:.4}°); re-anchored on {} teleports; {} zero-time commands carried into the next",
        stats.angle_mismatch,
        stats.angle_max_err_units,
        stats.angle_max_err_units as f32 * movement_iw4::SHORT2ANGLE,
        stats.reanchors,
        stats.zero_time,
    );
    println!(
        "limits yaw ±{yaw_limit} pitch ±{pitch_limit} deg/s: clipped {} yaw / {} pitch commands, cutting {:.1}° yaw / {:.1}° pitch in total",
        stats.yaw_clipped, stats.pitch_clipped, stats.yaw_cut_deg, stats.pitch_cut_deg,
    );
    Ok(())
}
