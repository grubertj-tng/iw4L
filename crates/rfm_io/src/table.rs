//! Reading the recorder's `movement.csv` back, for the offline tools.

use std::collections::HashMap;

use playerstate_iw4::PlayerState;

pub struct Header(HashMap<String, usize>);

impl Header {
    #[must_use]
    pub fn parse(line: &str) -> Self {
        Self(
            line.split(',')
                .enumerate()
                .map(|(i, name)| (name.to_string(), i))
                .collect(),
        )
    }

    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

pub struct Row {
    cols: Vec<String>,
}

impl Row {
    #[must_use]
    pub fn parse(line: &str) -> Self {
        Self {
            cols: line.split(',').map(str::to_string).collect(),
        }
    }

    #[must_use]
    pub fn get<'a>(&'a self, header: &Header, name: &str) -> Option<&'a str> {
        header
            .0
            .get(name)
            .and_then(|&i| self.cols.get(i))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    #[must_use]
    pub fn f32(&self, header: &Header, name: &str) -> Option<f32> {
        self.get(header, name).and_then(|v| v.parse().ok())
    }

    #[must_use]
    pub fn i64(&self, header: &Header, name: &str) -> Option<i64> {
        self.get(header, name).and_then(|v| {
            v.parse::<i64>()
                .ok()
                .or_else(|| v.parse::<f64>().ok().map(|f| f as i64))
        })
    }

    /// The movement-relevant part of the player state after the command.
    #[must_use]
    pub fn player_state(&self, header: &Header) -> PlayerState {
        let mut ps = PlayerState::ZERO;
        let int = |name: &str| self.i64(header, name).unwrap_or(0);
        let float = |name: &str| self.f32(header, name).unwrap_or(0.0);
        ps.pm_type = int("pm_type") as i32;
        ps.pm_flags = int("pm_flags") as u32;
        ps.velocity = [float("vel_x"), float("vel_y"), float("vel_z")];
        ps.viewangles = [float("view_pitch"), float("view_yaw"), 0.0];
        ps.delta_angles = [float("delta_pitch"), float("delta_yaw"), 0.0];
        ps.speed = int("speed") as i32;
        ps.move_speed_scale_multiplier = float("move_scale_mult");
        ps.weapon = int("weapon") as u32;
        ps.pm_time = int("pm_time") as i32;
        ps.damage_timer = int("damage_timer") as i32;
        ps.leanf = float("leanf");
        ps.f_weapon_pos_frac = float("weapon_pos_frac");
        ps.view_height_target = int("view_height_target") as i32;
        ps.view_height_lerp_time = int("view_height_lerp_time") as i32;
        ps.view_height_lerp_target = int("view_height_lerp_target") as i32;
        ps.view_height_lerp_down = int("view_height_lerp_down") as i32;
        ps.melee_charge_time = int("melee_charge_time") as i32;
        ps
    }
}
