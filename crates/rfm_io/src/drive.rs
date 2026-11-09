use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use bevy::prelude::*;
use math_iw4::angle_subtract;
use movement_iw4::{ANGLE2SHORT, SHORT2ANGLE, ViewAngleClamp, update_view_angles};
use net::{
    ClientCmdTemplate, ClientPredictionState, FrameClock, LocalPresentClient, LookState,
    PresentedSnapshot, WeaponSelect,
};
use playerstate_iw4::{PlayerState, UserCmd, buttons, pm_flags};

use crate::action::{
    Basis, IntendedTracker, MoveFrame, PITCH_RATE_LIMIT_DPS, YAW_RATE_LIMIT_DPS, angles_after_rate,
    moves, view_rate,
};
use crate::{FrameIndex, weapon_scales};

/// View pitch the sim clamps to. The adapter keeps the command inside it, so
/// the sim never rewrites `delta_angles` under the agent.
pub const PITCH_LIMIT_DEGREES: f32 = 85.0;

/// The sim refuses or ends a sprint at a `forwardmove` of 105 or less. While
/// sprint is desired the inverse picks among faster commands, or holding a
/// lower speed would end the sprint it was asked to keep.
pub const SPRINT_FORWARD_MIN: i8 = 106;

const VIEW_CLAMP: ViewAngleClamp = ViewAngleClamp {
    pitch_up: PITCH_LIMIT_DEGREES,
    pitch_down: PITCH_LIMIT_DEGREES,
    unclamped_pitch_bit: false,
};

/// Buttons an action holds directly. Sprint and stance are not among them:
/// the adapter derives those bits from `Action::sprint` and `Action::stance`.
pub const ACTION_BUTTONS: u32 = buttons::ATTACK
    | buttons::THROW
    | buttons::ADS
    | buttons::RELOAD
    | buttons::JUMP
    | buttons::USE
    | buttons::MELEE_CHARGE
    | buttons::FRAG
    | buttons::SMOKE
    | buttons::BREATH;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

impl Stance {
    #[must_use]
    pub fn of(ps: &PlayerState) -> Self {
        if ps.pm_flags & pm_flags::PRONE != 0 {
            Self::Prone
        } else if ps.pm_flags & pm_flags::CROUCH != 0 {
            Self::Crouch
        } else {
            Self::Stand
        }
    }

    #[must_use]
    pub fn buttons(self) -> u32 {
        match self {
            Self::Stand => 0,
            Self::Crouch => buttons::CROUCH,
            Self::Prone => buttons::PRONE,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Stand => "stand",
            Self::Crouch => "crouch",
            Self::Prone => "prone",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "stand" | "0" => Some(Self::Stand),
            "crouch" | "1" => Some(Self::Crouch),
            "prone" | "2" => Some(Self::Prone),
            _ => None,
        }
    }
}

/// One command's worth of action, in the interface's physical units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Action {
    /// Intended velocity after the command, u/s, `(forward, right)` in the
    /// player's yaw frame.
    pub intended: [f32; 2],
    /// Yaw rate, deg/s; positive turns left. Clamped to `YAW_RATE_LIMIT_DPS`.
    pub yaw_rate: f32,
    /// View pitch to reach by the end of the action, degrees; positive looks
    /// down. Clamped to ±`PITCH_LIMIT_DEGREES`, approached at most at
    /// `PITCH_RATE_LIMIT_DPS`.
    pub pitch: f32,
    pub sprint: bool,
    pub stance: Stance,
    /// Held buttons, out of [`ACTION_BUTTONS`].
    pub buttons: u32,
    /// Buttons that start a new activation on this command. One the previous
    /// command held is released for this command and pressed on the next.
    pub retrigger: u32,
    /// Weapon to select; `None` keeps the client's selection.
    pub weapon: Option<u16>,
}

/// The command the adapter is about to build.
#[derive(Clone, Copy, Debug)]
pub struct DriveTick {
    pub frame: u64,
    pub server_time: i32,
    pub msec: i32,
}

/// An action and the game time left until it ends, counted from the start of
/// the command's step. The adapter spreads the pitch change over that time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planned {
    pub action: Action,
    pub remaining_ms: i32,
}

/// Where actions come from. The adapter asks every frame while the player
/// moves, with the game time of the command it builds; a source answers with
/// the action valid at the start of that command's step (`server_time -
/// msec`), or `None` when it has no more.
pub trait ActionSource: Send + Sync + 'static {
    fn current(&mut self, tick: &DriveTick) -> Option<Planned>;

    /// Position of the current action in the source, for the drive log.
    fn cursor(&self) -> u64;
}

const LOG_HEADER: &str = "frame,cursor,server_time_ms,msec_built,msec_run,as_built,\
target_fwd,target_right,pitch_target,yaw_rate,pitch_rate_cmd,yaw_rate_cmd,\
sprint,stance,action_buttons,buttons,forwardmove,rightmove,cmd_pitch,cmd_yaw,\
before_pm_flags,guess_error";

struct Built {
    cmd: UserCmd,
    msec: i32,
    action: Action,
    rate: [f32; 2],
    deferred: u32,
    before_pm_flags: u32,
    guess_error: f32,
}

#[derive(Resource)]
pub struct Driver {
    source: Box<dyn ActionSource>,
    track: IntendedTracker,
    last_cmd: Option<UserCmd>,
    previous_move: (i8, i8),
    deferred: u32,
    built: Option<Built>,
    exhausted: bool,
    driven: u64,
    log: Option<BufWriter<File>>,
}

impl Driver {
    pub fn new(source: Box<dyn ActionSource>, log: Option<&Path>) -> std::io::Result<Self> {
        let log = match log {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out = BufWriter::new(File::create(path)?);
                writeln!(out, "{LOG_HEADER}")?;
                Some(out)
            }
            None => None,
        };
        Ok(Self {
            source,
            track: IntendedTracker::default(),
            last_cmd: None,
            previous_move: (0, 0),
            deferred: 0,
            built: None,
            exhausted: false,
            driven: 0,
            log,
        })
    }

    fn flush(&mut self) {
        if let Some(log) = self.log.as_mut()
            && let Err(error) = log.flush()
        {
            warn!("rfm_io: flushing the drive log failed: {error}");
        }
    }
}

fn view_pitch(pitch: i32, delta: f32) -> f32 {
    angle_subtract(pitch as f32 * SHORT2ANGLE + delta, 0.0)
}

/// Command pitch one step toward `target` (degrees): the remaining difference
/// shared over the action's remaining time, at most `PITCH_RATE_LIMIT_DPS`.
/// A step that ends the action lands on the target.
fn pitch_toward(current: i32, target: f32, delta: f32, msec: i32, remaining_ms: i32) -> i32 {
    let target_units = ((target - delta) * ANGLE2SHORT).round() as i32;
    let difference = i32::from(target_units.wrapping_sub(current) as i16);
    let share = if remaining_ms > msec {
        msec as f32 / remaining_ms as f32
    } else {
        1.0
    };
    let max_step = (PITCH_RATE_LIMIT_DPS * msec.max(0) as f32 * 0.001 * ANGLE2SHORT) as i32;
    let step = ((difference as f32 * share).round() as i32).clamp(-max_step, max_step);
    current.wrapping_add(step)
}

fn clamp_pitch(pitch: i32, delta: f32) -> i32 {
    let view = view_pitch(pitch, delta);
    let mut pitch = if view > PITCH_LIMIT_DEGREES {
        pitch - ((view - PITCH_LIMIT_DEGREES) * ANGLE2SHORT).ceil() as i32
    } else if view < -PITCH_LIMIT_DEGREES {
        pitch + ((-PITCH_LIMIT_DEGREES - view) * ANGLE2SHORT).ceil() as i32
    } else {
        return pitch;
    };
    while view_pitch(pitch, delta) > PITCH_LIMIT_DEGREES {
        pitch -= 1;
    }
    while view_pitch(pitch, delta) < -PITCH_LIMIT_DEGREES {
        pitch += 1;
    }
    pitch
}

/// The sim starts a sprint on a held `SPRINT` and cancels a running one on a
/// fresh press, so a running sprint keeps the previous bit and an unwanted one
/// is cancelled by release, then press.
fn sprint_bit(desired: bool, ps: &PlayerState, previous: u32) -> u32 {
    let held = previous & buttons::SPRINT;
    let sprinting = ps.pm_flags & pm_flags::SPRINTING != 0;
    match (desired, sprinting) {
        (true, true) => held,
        (true, false) if ps.sprint_button_up_required != 0 => 0,
        (true, false) => buttons::SPRINT,
        (false, true) => held ^ buttons::SPRINT,
        (false, false) => 0,
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[allow(clippy::too_many_arguments)]
pub fn drive_command(
    mut driver: ResMut<Driver>,
    frame: Res<FrameIndex>,
    template: Option<ResMut<ClientCmdTemplate>>,
    look: Option<ResMut<LookState>>,
    select: Option<ResMut<WeaponSelect>>,
    prediction: Option<Res<ClientPredictionState>>,
    clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
) {
    driver.built = None;
    let (Some(mut template), Some(mut look), Some(prediction), Some(clock)) =
        (template, look, prediction, clock)
    else {
        return;
    };
    let prediction = &prediction.0;
    if driver.exhausted || !template.ready || !prediction.is_armed() || !clock.started() {
        return;
    }
    let Some(ps) = prediction.world().player(prediction.local()).copied() else {
        return;
    };
    if !moves(&ps) {
        return;
    }
    let server_time = clock.time();
    let elapsed = server_time.wrapping_sub(ps.command_time);
    let msec = if elapsed > 0 { elapsed.min(200) } else { 0 };
    let previous = driver.last_cmd.unwrap_or(template.cmd);
    if msec == 0 {
        // The sim does not run this command; repeating the last one keeps
        // `old_buttons` and the angles where they are.
        let cmd = &mut template.cmd;
        cmd.buttons = previous.buttons;
        cmd.angles = previous.angles;
        cmd.forwardmove = previous.forwardmove;
        cmd.rightmove = previous.rightmove;
        look.angles = cmd.angles;
        return;
    }

    let tick = DriveTick {
        frame: frame.0,
        server_time,
        msec,
    };
    let Some(Planned {
        action,
        remaining_ms,
    }) = driver.source.current(&tick)
    else {
        driver.exhausted = true;
        driver.flush();
        info!(
            "rfm_io: action source exhausted after {} commands; the client's input drives again",
            driver.driven
        );
        return;
    };

    let mut cmd = template.cmd;
    cmd.server_time = server_time;
    let pitch_target = action
        .pitch
        .clamp(-PITCH_LIMIT_DEGREES, PITCH_LIMIT_DEGREES);
    let yaw_rate = action
        .yaw_rate
        .clamp(-YAW_RATE_LIMIT_DPS, YAW_RATE_LIMIT_DPS);
    let rate = [pitch_target, yaw_rate];
    let mut angles = angles_after_rate(previous.angles, [0.0, yaw_rate], msec);
    angles[0] = clamp_pitch(
        pitch_toward(
            previous.angles[0],
            pitch_target,
            ps.delta_angles[0],
            msec,
            remaining_ms,
        ),
        ps.delta_angles[0],
    );
    cmd.angles = angles;

    let world = prediction.world();
    let mut guess = ps;
    update_view_angles(&mut guess, &cmd, VIEW_CLAMP);
    let move_frame = MoveFrame {
        ps: &guess,
        server_time,
        msec,
        scales: weapon_scales(world, guess.weapon),
    };
    let target = Basis::from_yaw(guess.viewangles[1]).unproject(action.intended);
    let velocity = driver.track.velocity();
    let forward_min = if action.sprint {
        SPRINT_FORWARD_MIN
    } else {
        -127
    };
    let (forwardmove, rightmove) =
        move_frame.command_for_bounded(velocity, target, driver.previous_move, forward_min);
    let landed = move_frame
        .advance(velocity, forwardmove, rightmove)
        .velocity;
    cmd.forwardmove = forwardmove;
    cmd.rightmove = rightmove;

    let deferred = action.retrigger & previous.buttons & ACTION_BUTTONS;
    let held = ((action.buttons | action.retrigger) & ACTION_BUTTONS | driver.deferred) & !deferred;
    cmd.buttons = held | action.stance.buttons() | sprint_bit(action.sprint, &ps, previous.buttons);

    if let Some(weapon) = action.weapon {
        cmd.weapon = weapon;
        cmd.weapon_mapped = weapon;
        if let Some(mut select) = select
            && select.index != u32::from(weapon)
        {
            select.index = u32::from(weapon);
            select.mapped_index = u32::from(weapon);
            select.time = server_time;
        }
    }
    if cmd.buttons & (buttons::FRAG | buttons::SMOKE) != 0
        && let Some(loadout) = presented
            .as_deref()
            .and_then(PresentedSnapshot::snapshot)
            .zip(local.as_deref())
            .and_then(|(snapshot, local)| snapshot.meta.for_client(local.0))
            .and_then(|meta| meta.loadout.as_ref())
    {
        let slot = if cmd.buttons & buttons::FRAG != 0 {
            loadout.lethal
        } else {
            loadout.tactical
        };
        cmd.off_hand_index = slot as u16;
    }

    template.cmd = cmd;
    look.angles = cmd.angles;
    driver.built = Some(Built {
        cmd,
        msec,
        action,
        rate,
        deferred,
        before_pm_flags: ps.pm_flags,
        guess_error: distance(landed, target),
    });
}

/// After the predictor: if it ran a new command, the intended velocity steps
/// with it exactly as the recorder's does, and a command the adapter built
/// advances the source.
pub fn commit_command(
    mut driver: ResMut<Driver>,
    frame: Res<FrameIndex>,
    prediction: Option<Res<ClientPredictionState>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
) {
    let built = driver.built.take();
    let Some(prediction) = prediction.as_deref().map(|p| &p.0) else {
        return;
    };
    let Some(cmd) = prediction.last_cmd().copied() else {
        return;
    };
    if driver
        .last_cmd
        .is_some_and(|last| last.server_time == cmd.server_time)
    {
        return;
    }
    let msec = prediction.last_predict_msec().unwrap_or(0);
    let ps = prediction.predicted_local().copied().or_else(|| {
        presented
            .as_deref()
            .zip(local.as_deref())
            .and_then(|(presented, local)| presented.player(local.0).copied())
    });
    if let Some(ps) = ps {
        let scales = weapon_scales(prediction.world(), ps.weapon);
        driver.track.observe(&ps, &cmd, msec, Some(scales));
    }
    let previous = driver.last_cmd.replace(cmd);
    driver.previous_move = (cmd.forwardmove, cmd.rightmove);

    let Some(built) = built.filter(|built| built.cmd.server_time == cmd.server_time) else {
        driver.deferred = 0;
        return;
    };
    driver.deferred = built.deferred;
    driver.driven += 1;
    let cursor = driver.source.cursor();

    let Some(log) = driver.log.as_mut() else {
        return;
    };
    let commanded = previous.map_or([0.0, 0.0], |previous| view_rate(&previous, &cmd, msec));
    let action = built.action;
    let row = format!(
        "{},{cursor},{},{},{msec},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        frame.0,
        cmd.server_time,
        built.msec,
        u8::from(built.cmd == cmd),
        action.intended[0],
        action.intended[1],
        built.rate[0],
        built.rate[1],
        commanded[0],
        commanded[1],
        u8::from(action.sprint),
        action.stance.name(),
        action.buttons,
        cmd.buttons,
        cmd.forwardmove,
        cmd.rightmove,
        cmd.angles[0],
        cmd.angles[1],
        built.before_pm_flags,
        built.guess_error,
    );
    if let Err(error) = writeln!(log, "{row}") {
        warn!("rfm_io: writing the drive log failed: {error}");
    }
    if driver.driven.is_multiple_of(120) {
        driver.flush();
    }
}

pub fn flush_drive_on_exit(mut exit: MessageReader<AppExit>, mut driver: ResMut<Driver>) {
    if exit.read().next().is_some() {
        driver.flush();
    }
}
