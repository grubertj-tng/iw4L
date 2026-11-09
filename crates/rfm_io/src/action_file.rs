//! Actions as CSV in the interface's physical units. Each row holds from its
//! `t_ms` (game time, ms) until the next row's.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

use playerstate_iw4::buttons;

use crate::drive::{Action, ActionSource, DriveTick, Stance};

pub const BUTTON_COLUMNS: [(&str, u32); 10] = [
    ("attack", buttons::ATTACK),
    ("ads", buttons::ADS),
    ("reload", buttons::RELOAD),
    ("jump", buttons::JUMP),
    ("use", buttons::USE),
    ("melee", buttons::MELEE_CHARGE),
    ("frag", buttons::FRAG),
    ("smoke", buttons::SMOKE),
    ("breath", buttons::BREATH),
    ("throw", buttons::THROW),
];

#[must_use]
pub fn header() -> String {
    let mut header = String::from("src_frame,t_ms,fwd,right,pitch_rate,yaw_rate,sprint,stance");
    for (name, _) in BUTTON_COLUMNS {
        header.push(',');
        header.push_str(name);
    }
    header.push_str(",retrigger,weapon");
    header
}

#[must_use]
pub fn format_row(src_frame: u64, t_ms: i64, action: &Action) -> String {
    let mut row = format!(
        "{src_frame},{t_ms},{},{},{},{},{},{}",
        action.intended[0],
        action.intended[1],
        action.rate[0],
        action.rate[1],
        u8::from(action.sprint),
        action.stance.name(),
    );
    for (_, bit) in BUTTON_COLUMNS {
        row.push(',');
        row.push(if action.buttons & bit != 0 { '1' } else { '0' });
    }
    row.push_str(&format!(
        ",{},{}",
        action.retrigger,
        action.weapon.unwrap_or(0)
    ));
    row
}

fn parse_rows(text: impl BufRead) -> Result<Vec<(i64, Action)>, String> {
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or("empty action file")?
        .map_err(|e| e.to_string())?;
    let index: HashMap<&str, usize> = header
        .split(',')
        .enumerate()
        .map(|(i, name)| (name.trim(), i))
        .collect();
    for required in ["t_ms", "fwd", "right", "pitch_rate", "yaw_rate"] {
        if !index.contains_key(required) {
            return Err(format!("missing column {required}"));
        }
    }
    let mut actions = Vec::new();
    for (number, line) in lines.enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let at = |name: &str| {
            index
                .get(name)
                .and_then(|&i| cols.get(i))
                .copied()
                .filter(|v| !v.is_empty())
        };
        let row = number + 2;
        let float = |name: &str| -> Result<f32, String> {
            at(name).map_or(Ok(0.0), |v| {
                v.parse()
                    .map_err(|_| format!("line {row}: bad {name} `{v}`"))
            })
        };
        let int = |name: &str| -> Result<u32, String> {
            at(name).map_or(Ok(0), |v| {
                v.parse()
                    .map_err(|_| format!("line {row}: bad {name} `{v}`"))
            })
        };
        let stance = match at("stance") {
            Some(v) => Stance::parse(v).ok_or(format!("line {row}: bad stance `{v}`"))?,
            None => Stance::Stand,
        };
        let mut held = 0;
        for (name, bit) in BUTTON_COLUMNS {
            if int(name)? != 0 {
                held |= bit;
            }
        }
        let weapon = int("weapon")?;
        let t_ms: i64 = at("t_ms")
            .ok_or(format!("line {row}: missing t_ms"))?
            .parse()
            .map_err(|_| format!("line {row}: bad t_ms"))?;
        if actions.last().is_some_and(|(last, _)| *last > t_ms) {
            return Err(format!("line {row}: t_ms goes backwards"));
        }
        actions.push((
            t_ms,
            Action {
                intended: [float("fwd")?, float("right")?],
                rate: [float("pitch_rate")?, float("yaw_rate")?],
                sprint: int("sprint")? != 0,
                stance,
                buttons: held,
                retrigger: int("retrigger")?,
                weapon: (weapon != 0).then_some(weapon as u16),
            },
        ));
    }
    Ok(actions)
}

/// Replays an action file by game time: a command gets the row valid at the
/// start of its step, counted from the first command driven. Durations follow
/// game time whatever the frame rate.
pub struct ActionFile {
    actions: Vec<(i64, Action)>,
    cursor: usize,
    origin: Option<i64>,
}

impl ActionFile {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let actions = parse_rows(std::io::BufReader::new(file))?;
        Ok(Self {
            actions,
            cursor: 0,
            origin: None,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl ActionSource for ActionFile {
    fn current(&mut self, tick: &DriveTick) -> Option<Action> {
        let (first, _) = *self.actions.first()?;
        let step_start = i64::from(tick.server_time) - i64::from(tick.msec);
        let origin = *self.origin.get_or_insert(step_start - first);
        let t = step_start - origin;
        while self
            .actions
            .get(self.cursor + 1)
            .is_some_and(|(next, _)| *next <= t)
        {
            self.cursor += 1;
        }
        let (last, _) = *self.actions.last()?;
        let tail = match self.actions.len() {
            n if n >= 2 => last - self.actions[n - 2].0,
            _ => 0,
        };
        if self.cursor + 1 == self.actions.len() && t >= last + tail.max(1) {
            return None;
        }
        Some(self.actions[self.cursor].1)
    }

    fn cursor(&self) -> u64 {
        self.cursor as u64
    }
}
