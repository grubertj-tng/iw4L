use movement_iw4::{
    ANGLE2SHORT, CmdScaleWalkContext, Pml, SHORT2ANGLE, StanceSurface, accelerate, cmd_scale_walk,
    damage_scale_walk, friction, stance_surface_type,
};
use playerstate_iw4::{PlayerState, UserCmd, pm_flags};

const BACK_SPEED_SCALE: f32 = 0.7;
const STRAFE_SPEED_SCALE: f32 = 0.8;
const SPRINT_SPEED_SCALE: f32 = 1.5;
const LAST_STAND_CRAWL_SPEED_SCALE: f32 = 0.15;

const ACCEL_STAND: f32 = 9.0;
const ACCEL_CROUCH: f32 = 12.0;
const ACCEL_PRONE: f32 = 19.0;
const ACCEL_MIN_WISH_SPEED: f32 = 100.0;

const SEARCH_RADIUS: i32 = 2;

pub const YAW_RATE_LIMIT_DPS: f32 = 1200.0;
pub const PITCH_RATE_LIMIT_DPS: f32 = 240.0;

pub const EXCLUDED_FLAGS: u32 = pm_flags::LADDER
    | pm_flags::MANTLE
    | pm_flags::JUMPING
    | pm_flags::TIME_HARDLANDING
    | pm_flags::MELEE_CHARGE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveScales {
    pub weapon_move: f32,
    pub weapon_ads_move: f32,
    pub shellshock_affects_movement: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Basis {
    pub forward: [f32; 2],
    pub right: [f32; 2],
}

impl Basis {
    #[must_use]
    pub fn from_yaw(yaw_degrees: f32) -> Self {
        let (forward, right, _) = math_iw4::angle_vectors([0.0, yaw_degrees, 0.0]);
        Self {
            forward: [forward[0], forward[1]],
            right: [right[0], right[1]],
        }
    }

    #[must_use]
    pub fn project(&self, v: [f32; 2]) -> [f32; 2] {
        [
            v[0] * self.forward[0] + v[1] * self.forward[1],
            v[0] * self.right[0] + v[1] * self.right[1],
        ]
    }

    #[must_use]
    pub fn unproject(&self, v: [f32; 2]) -> [f32; 2] {
        [
            v[0] * self.forward[0] + v[1] * self.right[0],
            v[0] * self.forward[1] + v[1] * self.right[1],
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntendedStep {
    pub wish_speed: f32,
    pub velocity: [f32; 2],
}

/// The movement state a command moved with: the player state after the sim
/// ran it (stance, sprint, view angles), the weapon's move scales and the step
/// length.
#[derive(Clone, Copy, Debug)]
pub struct MoveFrame<'a> {
    pub ps: &'a PlayerState,
    pub server_time: i32,
    pub msec: i32,
    pub scales: MoveScales,
}

impl MoveFrame<'_> {
    fn command(&self, forwardmove: i8, rightmove: i8) -> UserCmd {
        UserCmd {
            server_time: self.server_time,
            forwardmove,
            rightmove,
            ..UserCmd::default()
        }
    }

    /// Speed the sim aims for under `forwardmove/rightmove`, in units/s.
    #[must_use]
    pub fn wish_speed(&self, forwardmove: i8, rightmove: i8) -> f32 {
        let context = CmdScaleWalkContext {
            player_back_speed_scale: BACK_SPEED_SCALE,
            player_strafe_speed_scale: STRAFE_SPEED_SCALE,
            player_sprint_speed_scale: SPRINT_SPEED_SCALE,
            player_last_stand_crawl_speed_scale: LAST_STAND_CRAWL_SPEED_SCALE,
            weapon_move_speed_scale: self.scales.weapon_move,
            weapon_ads_move_speed_scale: self.scales.weapon_ads_move,
            shellshock_affects_movement: self.scales.shellshock_affects_movement,
        };
        let cmd = self.command(forwardmove, rightmove);
        let f = f32::from(forwardmove);
        let r = f32::from(rightmove);
        (f * f + r * r).sqrt()
            * cmd_scale_walk(self.ps, &cmd, context)
            * damage_scale_walk(self.ps.damage_timer)
    }

    /// Wish speed of a full forward command: the ceiling the current stance,
    /// sprint, weapon and perks allow.
    #[must_use]
    pub fn full_speed(&self) -> f32 {
        self.wish_speed(127, 0)
    }

    fn pml(&self) -> Pml {
        flat_ground_pml(self.msec.clamp(1, 200))
    }

    fn virtual_ps(&self, velocity: [f32; 2]) -> PlayerState {
        let mut ps = *self.ps;
        ps.pm_flags &= !EXCLUDED_FLAGS;
        ps.velocity = [velocity[0], velocity[1], 0.0];
        ps
    }

    fn after_friction(&self, velocity: [f32; 2]) -> [f32; 2] {
        let mut ps = self.virtual_ps(velocity);
        friction(&mut ps, &self.pml());
        [ps.velocity[0], ps.velocity[1]]
    }

    fn accel(&self) -> f32 {
        match stance_surface_type(self.ps) {
            StanceSurface::Prone | StanceSurface::LastStand => ACCEL_PRONE,
            StanceSurface::Crouch => ACCEL_CROUCH,
            StanceSurface::Stand => ACCEL_STAND,
        }
    }

    /// One command on virtual flat ground: the sim's friction, then its
    /// acceleration toward the command's wish velocity.
    #[must_use]
    pub fn advance(&self, velocity: [f32; 2], forwardmove: i8, rightmove: i8) -> IntendedStep {
        let pml = self.pml();
        let mut ps = self.virtual_ps(velocity);
        friction(&mut ps, &pml);

        let basis = Basis::from_yaw(self.ps.viewangles[1]);
        let f = f32::from(forwardmove);
        let r = f32::from(rightmove);
        let mut wishdir = [
            f * basis.forward[0] + r * basis.right[0],
            f * basis.forward[1] + r * basis.right[1],
            0.0,
        ];
        let length = (wishdir[0] * wishdir[0] + wishdir[1] * wishdir[1]).sqrt();
        if length > 0.0 {
            wishdir[0] /= length;
            wishdir[1] /= length;
        }
        let wish_speed = self.wish_speed(forwardmove, rightmove);
        accelerate(&mut ps, &pml, &wishdir, wish_speed, self.accel());
        IntendedStep {
            wish_speed,
            velocity: [ps.velocity[0], ps.velocity[1]],
        }
    }

    /// The `forwardmove/rightmove` whose step from `velocity` lands closest to
    /// `target`. Solves the acceleration step for the wish speed, then
    /// searches the neighbouring integer commands, because the command is
    /// quantised to `i8`. Above the wish speed several commands land on the
    /// same velocity; `previous` wins those ties, so a held key stays held.
    #[must_use]
    pub fn command_for(
        &self,
        velocity: [f32; 2],
        target: [f32; 2],
        previous: (i8, i8),
    ) -> (i8, i8) {
        let analytic = self.analytic_command(velocity, target);
        let error = |cmd: (i8, i8)| {
            let next = self.advance(velocity, cmd.0, cmd.1).velocity;
            let dx = next[0] - target[0];
            let dy = next[1] - target[1];
            dx * dx + dy * dy
        };
        let mut best = previous;
        let mut best_error = error(previous);
        for candidate in [analytic, (0, 0)] {
            let e = error(candidate);
            if e < best_error {
                best = candidate;
                best_error = e;
            }
        }
        for df in -SEARCH_RADIUS..=SEARCH_RADIUS {
            for dr in -SEARCH_RADIUS..=SEARCH_RADIUS {
                let candidate = (
                    clamp_i8(i32::from(analytic.0) + df),
                    clamp_i8(i32::from(analytic.1) + dr),
                );
                let e = error(candidate);
                if e < best_error {
                    best = candidate;
                    best_error = e;
                }
            }
        }
        best
    }

    fn analytic_command(&self, velocity: [f32; 2], target: [f32; 2]) -> (i8, i8) {
        let after = self.after_friction(velocity);
        let d = [target[0] - after[0], target[1] - after[1]];
        let d_len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if d_len < 1e-3 {
            return (0, 0);
        }
        let dir = [d[0] / d_len, d[1] / d_len];
        let basis = Basis::from_yaw(self.ps.viewangles[1]);
        let local = basis.project(dir);
        let largest = local[0].abs().max(local[1].abs());
        if largest <= 0.0 {
            return (0, 0);
        }
        let full = (
            clamp_i8((local[0] / largest * 127.0).round() as i32),
            clamp_i8((local[1] / largest * 127.0).round() as i32),
        );
        let full_wish = self.wish_speed(full.0, full.1);
        if full_wish <= 0.0 {
            return (0, 0);
        }

        let dt = self.msec.clamp(1, 200) as f32 * 0.001;
        let along = after[0] * dir[0] + after[1] * dir[1];
        let accel_step = |wish: f32| self.accel() * wish.max(ACCEL_MIN_WISH_SPEED) * dt;
        let capped = d_len + along;
        let wish = if capped > 0.0 && accel_step(capped) >= d_len {
            capped
        } else {
            d_len / (self.accel() * dt)
        };

        let k = (wish / full_wish).min(1.0);
        (
            clamp_i8((f32::from(full.0) * k).round() as i32),
            clamp_i8((f32::from(full.1) * k).round() as i32),
        )
    }
}

/// The velocity the player's movement commands would produce on unobstructed
/// flat ground: the sim's own friction and acceleration, run on a velocity of
/// its own that collision, slopes, gravity and knockback never touch. Jump
/// landings, hard landings and melee lunges belong to those actions, not to
/// movement, and are left out ([`EXCLUDED_FLAGS`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntendedVelocity {
    velocity: [f32; 2],
}

impl IntendedVelocity {
    pub fn reset(&mut self, velocity: [f32; 2]) {
        self.velocity = velocity;
    }

    #[must_use]
    pub fn velocity(&self) -> [f32; 2] {
        self.velocity
    }

    pub fn step(&mut self, frame: &MoveFrame<'_>, cmd: &UserCmd) -> IntendedStep {
        let step = frame.advance(self.velocity, cmd.forwardmove, cmd.rightmove);
        self.velocity = step.velocity;
        step
    }
}

fn clamp_i8(value: i32) -> i8 {
    value.clamp(-127, 127) as i8
}

fn flat_ground_pml(msec: i32) -> Pml {
    let mut ground_trace = [0u32; 11];
    ground_trace[3] = 1.0f32.to_bits();
    Pml {
        forward: [0.0; 3],
        right: [0.0; 3],
        up: [0.0; 3],
        frametime: msec as f32 * 0.001,
        msec,
        walking: 1,
        ground_plane: 1,
        almost_ground_plane: 1,
        ground_trace,
        previous_origin: [0.0; 3],
        previous_velocity: [0.0; 3],
        holdrand: 0,
        jump_animations: [None; 4],
        mantle_movetype: None,
        landing_animation: false,
        fall_damage: 0,
    }
}

/// Rotation between two commands in degrees per second, `[pitch, yaw]`.
/// Positive pitch looks down, positive yaw turns left.
#[must_use]
pub fn view_rate(previous: &UserCmd, next: &UserCmd, msec: i32) -> [f32; 2] {
    if msec <= 0 {
        return [0.0, 0.0];
    }
    let seconds = msec as f32 * 0.001;
    let delta = |axis: usize| {
        let units = next.angles[axis].wrapping_sub(previous.angles[axis]) as i16;
        f32::from(units) * SHORT2ANGLE / seconds
    };
    [delta(0), delta(1)]
}

/// Command angles after rotating at `rate` (deg/s, `[pitch, yaw]`) for `msec`.
#[must_use]
pub fn angles_after_rate(previous: [i32; 3], rate: [f32; 2], msec: i32) -> [i32; 3] {
    let seconds = msec.max(0) as f32 * 0.001;
    let units = |axis: usize| (rate[axis] * seconds * ANGLE2SHORT).round() as i32;
    [
        previous[0].wrapping_add(units(0)),
        previous[1].wrapping_add(units(1)),
        previous[2],
    ]
}
