use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use bevy::prelude::*;
use net::{ClientPredictionState, FrameClock, LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::{ENTITYNUM_NONE, UserCmd};

use crate::action::{Basis, IntendedTracker, RateTracker};
use crate::{FrameIndex, weapon_scales};

const HEADER: &str = "frame,perf_frame,real_s,cg_time_ms,presented_tick,\
input_captured,console_or_chat,script_menu,ui_overlay,window_focused,\
cmd_new,server_time_ms,cmd_msec,buttons,forwardmove,rightmove,cmd_pitch,cmd_yaw,cmd_weapon,\
pitch_rate_dps,yaw_rate_dps,\
ps_valid,pm_type,pm_flags,on_ground,origin_x,origin_y,origin_z,vel_x,vel_y,vel_z,\
view_pitch,view_yaw,delta_pitch,delta_yaw,speed,move_scale_mult,weapon,\
weapon_move_scale,weapon_ads_move_scale,\
pm_time,damage_timer,leanf,weapon_pos_frac,view_height_target,\
view_height_lerp_time,view_height_lerp_target,view_height_lerp_down,melee_charge_time,full_speed,\
wish_speed,intended_vx,intended_vy,intended_fwd,intended_right,actual_fwd,actual_right";

const FLUSH_EVERY: u64 = 120;

#[derive(Resource)]
pub struct MovementRecorder {
    out: BufWriter<File>,
    rows: u64,
    last_cmd: Option<UserCmd>,
    last_delta_angles: Option<[f32; 3]>,
    track: IntendedTracker,
    rate: [f32; 2],
    rates: RateTracker,
}

impl MovementRecorder {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "{HEADER}")?;
        Ok(Self {
            out,
            rows: 0,
            last_cmd: None,
            last_delta_angles: None,
            track: IntendedTracker::default(),
            rate: [0.0, 0.0],
            rates: RateTracker::default(),
        })
    }

    pub fn flush(&mut self) {
        if let Err(error) = self.out.flush() {
            warn!("rfm_io: flushing the movement recording failed: {error}");
        }
    }
}

fn horizontal(v: [f32; 3]) -> [f32; 2] {
    [v[0], v[1]]
}

#[allow(clippy::too_many_arguments)]
/// Whether something other than the game has the player's input: the
/// console or chat, a script menu, a UI overlay, or another window.
#[derive(bevy::ecs::system::SystemParam)]
pub struct InputCapture<'w> {
    hud: Option<Res<'w, frame::HudInputView>>,
    menus: hud::MenuState<'w>,
    overlay: Option<Res<'w, frame::UiOverlayInput>>,
    devices: Option<Res<'w, frame::InputDevices>>,
}

#[allow(clippy::too_many_arguments)]
pub fn record_frame(
    mut rec: ResMut<MovementRecorder>,
    frame: Res<FrameIndex>,
    real: Res<Time<Real>>,
    clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    prediction: Option<Res<ClientPredictionState>>,
    capture: InputCapture,
) {
    let prediction = prediction.as_deref().map(|p| &p.0);
    let ps = prediction
        .and_then(|p| p.predicted_local())
        .copied()
        .or_else(|| {
            presented
                .as_deref()
                .zip(local.as_deref())
                .and_then(|(presented, local)| presented.player(local.0).copied())
        });
    let cmd = prediction.and_then(|p| p.last_cmd()).copied();
    let msec = prediction.and_then(|p| p.last_predict_msec()).unwrap_or(0);

    let cmd_new = match (cmd, rec.last_cmd) {
        (Some(cmd), Some(last)) => cmd.server_time != last.server_time,
        (Some(_), None) => true,
        (None, _) => false,
    };

    if cmd_new && let (Some(cmd), Some(ps)) = (cmd, ps) {
        let teleported = rec
            .last_delta_angles
            .is_some_and(|delta| delta != ps.delta_angles);
        rec.rate = rec.rates.observe(cmd.angles, msec, teleported);
        let scales = prediction.map(|p| weapon_scales(p.world(), ps.weapon));
        rec.track.observe(&ps, &cmd, msec, scales);
        rec.last_delta_angles = Some(ps.delta_angles);
    }
    if cmd_new {
        rec.last_cmd = cmd;
    }

    let mut row = String::with_capacity(512);
    let mut push = |value: &dyn std::fmt::Display| {
        if !row.is_empty() {
            row.push(',');
        }
        row.push_str(&value.to_string());
    };
    let blank = "";

    push(&frame.0);
    push(&perf::frames::open_index());
    push(&format_args!("{:.6}", real.elapsed_secs_f64()));
    match clock.as_deref() {
        Some(clock) if clock.started() => push(&clock.time()),
        _ => push(&blank),
    }
    match presented.as_deref().and_then(PresentedSnapshot::tick) {
        Some(tick) => push(&tick.0),
        None => push(&blank),
    }

    let console_or_chat = capture.hud.as_deref().is_some_and(|h| h.console_open);
    let script_menu = capture.menus.is_some_and(hud::ScriptMenus::captures_input);
    let ui_overlay = capture.overlay.as_deref().is_some_and(|o| o.0);
    let window_focused = capture.devices.as_deref().is_none_or(|d| d.focused);
    push(&u8::from(
        console_or_chat || script_menu || ui_overlay || !window_focused,
    ));
    push(&u8::from(console_or_chat));
    push(&u8::from(script_menu));
    push(&u8::from(ui_overlay));
    push(&u8::from(window_focused));
    push(&u8::from(cmd_new));
    match cmd {
        Some(cmd) => {
            push(&cmd.server_time);
            push(&msec);
            push(&cmd.buttons);
            push(&cmd.forwardmove);
            push(&cmd.rightmove);
            push(&cmd.angles[0]);
            push(&cmd.angles[1]);
            push(&cmd.weapon);
        }
        None => {
            for _ in 0..8 {
                push(&blank);
            }
        }
    }
    push(&rec.rate[0]);
    push(&rec.rate[1]);

    match ps {
        Some(ps) => {
            push(&1);
            push(&ps.pm_type);
            push(&ps.pm_flags);
            push(&u8::from(ps.ground_entity_num != ENTITYNUM_NONE));
            for v in ps.origin {
                push(&v);
            }
            for v in ps.velocity {
                push(&v);
            }
            push(&ps.viewangles[0]);
            push(&ps.viewangles[1]);
            push(&ps.delta_angles[0]);
            push(&ps.delta_angles[1]);
            push(&ps.speed);
            push(&ps.move_speed_scale_multiplier);
            push(&ps.weapon);
            let scales = rec.track.scales();
            push(&scales.weapon_move);
            push(&scales.weapon_ads_move);
            push(&ps.pm_time);
            push(&ps.damage_timer);
            push(&ps.leanf);
            push(&ps.f_weapon_pos_frac);
            push(&ps.view_height_target);
            push(&ps.view_height_lerp_time);
            push(&ps.view_height_lerp_target);
            push(&ps.view_height_lerp_down);
            push(&ps.melee_charge_time);
            let step = rec.track.step();
            push(&rec.track.full_speed());
            push(&step.wish_speed);
            push(&step.velocity[0]);
            push(&step.velocity[1]);
            let basis = Basis::from_yaw(ps.viewangles[1]);
            let intended = basis.project(step.velocity);
            let actual = basis.project(horizontal(ps.velocity));
            push(&intended[0]);
            push(&intended[1]);
            push(&actual[0]);
            push(&actual[1]);
        }
        None => {
            push(&0);
            for _ in 0..35 {
                push(&blank);
            }
        }
    }

    if let Err(error) = writeln!(rec.out, "{row}") {
        warn!("rfm_io: writing the movement recording failed: {error}");
    }
    rec.rows += 1;
    if rec.rows.is_multiple_of(FLUSH_EVERY) {
        rec.flush();
    }
}

pub fn flush_on_exit(mut exit: MessageReader<AppExit>, mut rec: ResMut<MovementRecorder>) {
    if exit.read().next().is_some() {
        rec.flush();
    }
}
