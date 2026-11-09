//! Action file from a movement recording: one row per command the player
//! moved with, in the interface's physical units, for `IW4L_RFM_DRIVE`.
//!
//! usage: rfm_actions <movement.csv> <actions.csv>
//!
//! * intended velocity: the recorded `intended_fwd/right`;
//! * rates: the recorded `pitch/yaw_rate_dps`;
//! * sprint and stance desired: the recorded `SPRINTING`, `PRONE`, `CROUCH`
//!   state after the command;
//! * buttons: the recorded bits out of `ACTION_BUTTONS`;
//! * weapon: 0, the client's selection stays.
//!
//! Commands while dead or spectating are skipped.

use std::io::{BufRead, BufReader, BufWriter, Write};

use playerstate_iw4::{buttons, pm_flags};
use rfm_io::action::{PITCH_RATE_LIMIT_DPS, YAW_RATE_LIMIT_DPS, moves};
use rfm_io::action_file::{format_row, header};
use rfm_io::drive::{ACTION_BUTTONS, Action, Stance};
use rfm_io::table::{Header, Row};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: rfm_actions <movement.csv> <actions.csv>");
        std::process::exit(2);
    }
    let mut lines = BufReader::new(std::fs::File::open(&args[1])?).lines();
    let Some(first) = lines.next().transpose()? else {
        eprintln!("empty recording");
        std::process::exit(2);
    };
    let ix = Header::parse(&first);
    let mut out = BufWriter::new(std::fs::File::create(&args[2])?);
    writeln!(out, "{}", header())?;

    let derived = buttons::SPRINT | buttons::PRONE | buttons::CROUCH | buttons::STANCE_HELD;
    let (mut written, mut skipped, mut dropped_bits, mut clipped) = (0u64, 0u64, 0u64, 0u64);
    let (mut sprinting, mut crouched, mut prone) = (0u64, 0u64, 0u64);
    for line in lines {
        let row = Row::parse(&line?);
        if row.i64(&ix, "cmd_new") != Some(1) || row.i64(&ix, "ps_valid") != Some(1) {
            continue;
        }
        let ps = row.player_state(&ix);
        if !moves(&ps) {
            skipped += 1;
            continue;
        }
        let recorded = row.i64(&ix, "buttons").unwrap_or(0) as u32;
        if recorded & !(ACTION_BUTTONS | derived) != 0 {
            dropped_bits += 1;
        }
        let action = Action {
            intended: [
                row.f32(&ix, "intended_fwd").unwrap_or(0.0),
                row.f32(&ix, "intended_right").unwrap_or(0.0),
            ],
            rate: [
                row.f32(&ix, "pitch_rate_dps").unwrap_or(0.0),
                row.f32(&ix, "yaw_rate_dps").unwrap_or(0.0),
            ],
            sprint: ps.pm_flags & pm_flags::SPRINTING != 0,
            stance: Stance::of(&ps),
            buttons: recorded & ACTION_BUTTONS,
            retrigger: 0,
            weapon: None,
        };
        if action.rate[0].abs() > PITCH_RATE_LIMIT_DPS || action.rate[1].abs() > YAW_RATE_LIMIT_DPS
        {
            clipped += 1;
        }
        sprinting += u64::from(action.sprint);
        crouched += u64::from(action.stance == Stance::Crouch);
        prone += u64::from(action.stance == Stance::Prone);
        let src_frame = row.i64(&ix, "frame").unwrap_or(0) as u64;
        writeln!(out, "{}", format_row(src_frame, &action))?;
        written += 1;
    }
    out.flush()?;
    println!("actions: {written}  (skipped {skipped} commands while not moving)");
    println!("sprint desired {sprinting}, crouch {crouched}, prone {prone}");
    println!("commands with recorded bits outside the action buttons: {dropped_bits}");
    println!("commands with a rate beyond the limits (the adapter clamps): {clipped}");
    Ok(())
}
