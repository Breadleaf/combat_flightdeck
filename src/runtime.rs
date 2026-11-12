use std::collections::{BTreeMap, HashMap, HashSet};

use gilrs::{Axis, Button};

use crate::config::{
    Behavior, Config, Drive, Edge, Gain, Input, Output, Press, Reverse, SkidSource, Slew,
};

pub const NEUTRAL_US: f32 = 1500.0;
pub const HALF_SPAN_US: f32 = 500.0;

#[derive(Default)]
pub struct Inputs {
    pub connected: bool,
    pub axes: HashMap<Axis, f32>,
    pub held: HashSet<Button>,
    pub pressed: HashSet<Button>,
}

struct GateState {
    on: bool,
    held_ms: f32,
    // true once hold is flipped, cleared when chord is released
    fired: bool,
}

pub struct Runtime<'a> {
    config: &'a Config,
    gates: BTreeMap<&'a str, GateState>,
    behaviors: BTreeMap<&'a str, f32>,
    outputs: BTreeMap<&'a str, f32>,
    feeds: BTreeMap<&'a str, Vec<&'a str>>,
    disconnected_ms: f32,
}

fn behavior_init(b: &Behavior) -> f32 {
    match b {
        Behavior::Toggle { init, .. }
        | Behavior::Select { init, .. }
        | Behavior::Step { init, .. } => *init,
        Behavior::Hold { default, .. } => *default,
    }
}

fn button_of(config: &Config, name: &str) -> Option<Button> {
    match config.inputs.get(name) {
        Some(Input::Button { source }) => Some(*source),
        _ => None,
    }
}

fn advance_behavior(config: &Config, b: &Behavior, current: f32, input: &Inputs) -> f32 {
    let pressed = |name: &str| button_of(config, name).is_some_and(|b| input.pressed.contains(&b));
    let held = |name: &str| button_of(config, name).is_some_and(|b| input.held.contains(&b));
    let hit = |p: &Press| matches!(p, Press::On(n) if pressed(n));

    match b {
        Behavior::Toggle { on, a, b, .. } => {
            if pressed(on) {
                if current == *a { *b } else { *a }
            } else {
                current
            }
        }
        Behavior::Hold { on, value, default } => {
            if held(on) {
                *value
            } else {
                *default
            }
        }
        Behavior::Select { options, .. } => {
            let mut v = current;
            for o in options {
                if pressed(&o.on) {
                    v = o.value;
                }
            }
            v
        }
        Behavior::Step {
            up,
            down,
            reset,
            by,
            min,
            max,
            at_min,
            at_max,
            reset_to,
            ..
        } => {
            let mut v = current;
            if hit(up) {
                let n = v + by;
                v = if n > *max {
                    match at_max {
                        Edge::Clamp => *max,
                        Edge::Wrap(to) => *to,
                    }
                } else {
                    n
                };
            }
            if hit(down) {
                let n = v - by;
                v = if n < *min {
                    match at_min {
                        Edge::Clamp => *min,
                        Edge::Wrap(to) => *to,
                    }
                } else {
                    n
                };
            }
            if hit(reset) {
                v = *reset_to;
            }
            v
        }
    }
}

fn to_us(v: f32) -> f32 {
    NEUTRAL_US + v.clamp(-1.0, 1.0) * HALF_SPAN_US
}

fn mix<'c>(config: &'c Config, signals: &BTreeMap<&str, f32>) -> Vec<(&'c str, f32)> {
    // validate() ensures every name exists, so 0.0 is never actually used
    let get = |n: &str| signals.get(n).copied().unwrap_or(0.0);
    match &config.drive {
        Drive::Skid {
            source,
            left,
            right,
            gain,
            reversed_when,
            normalize,
        } => {
            let flipped = match reversed_when {
                Reverse::Never => false,
                Reverse::When(n) => get(n) > 0.5,
            };
            let (mut l, mut r) = match source {
                SkidSource::Arcade { throttle, steer } => {
                    let t = if flipped {
                        -get(throttle)
                    } else {
                        get(throttle)
                    };
                    let s = get(steer);
                    (t + s, t - s)
                }
                SkidSource::Tank { left, right } => (get(left), get(right)),
            };
            if *normalize {
                let m = l.abs().max(r.abs()).max(1.0);
                l /= m;
                r /= m;
            }
            let g = match gain {
                Gain::Fixed(g) => *g,
                Gain::From(n) => get(n),
            };
            l *= g;
            r *= g;
            left.iter()
                .map(|n| (n.as_str(), to_us(l)))
                .chain(right.iter().map(|n| (n.as_str(), to_us(r))))
                .collect()
        }
        Drive::Matrix {
            inputs,
            rows,
            normalize,
        } => {
            let mut out: Vec<(&str, f32)> = rows
                .iter()
                .map(|(name, row)| {
                    (
                        name.as_str(),
                        row.iter().zip(inputs).map(|(c, i)| c * get(i)).sum::<f32>(),
                    )
                })
                .collect();
            if *normalize {
                let m = out.iter().map(|(_, v)| v.abs()).fold(1.0, f32::max);
                for (_, v) in &mut out {
                    *v /= m;
                }
            }
            out.into_iter().map(|(n, v)| (n, to_us(v))).collect()
        }
    }
}

impl<'a> Runtime<'a> {
    // this is the onlt way to get a runtime object; this ensures a invalid config, cannot become a runtime
    pub fn new(config: &'a Config) -> Result<Self, Vec<String>> {
        crate::validate::validate(config)?;
        let gates = config
            .gates
            .keys()
            .map(|n| {
                (
                    n.as_str(),
                    GateState {
                        on: false,
                        held_ms: 0.0,
                        fired: true,
                    },
                )
            })
            .collect();
        let behaviors = config
            .behaviors
            .iter()
            .map(|(n, b)| (n.as_str(), behavior_init(b)))
            .collect();
        let outputs = config
            .outputs
            .iter()
            .map(|(n, o)| {
                let Output::Motor { init, .. } = o;
                (n.as_str(), *init as f32)
            })
            .collect();
        let mut feeds: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (n, o) in &config.outputs {
            let Output::Motor { source, .. } = o;
            if config.behaviors.contains_key(source) {
                feeds.entry(source.as_str()).or_default().push(n.as_str());
            }
        }
        Ok(Runtime {
            config,
            gates,
            behaviors,
            outputs,
            feeds,
            disconnected_ms: 0.0,
        })
    }

    pub fn watched_buttons(&self) -> Vec<Button> {
        let mut v: Vec<Button> = Vec::new();
        for i in self.config.inputs.values() {
            if let Input::Button { source } = i {
                v.push(*source)
            }
        }
        for g in self.config.gates.values() {
            v.extend(g.chord.iter().copied());
        }
        let mut seen: HashSet<Button> = HashSet::new();
        v.retain(|b| seen.insert(*b));
        v
    }

    pub fn watched_axis(&self) -> Vec<Axis> {
        let mut v: Vec<Axis> = Vec::new();
        for i in self.config.inputs.values() {
            if let Input::Axis { source, .. } = i {
                if !v.contains(source) {
                    v.push(*source);
                }
            }
        }
        v
    }

    pub fn gates_on(&self) -> Vec<String> {
        self.gates
            .iter()
            .filter(|(_, g)| g.on)
            .map(|(n, _)| n.to_string())
            .collect()
    }

    fn update_gates(&mut self, input: &Inputs, dt_ms: f32) {
        for (n, g) in &self.config.gates {
            let st = self
                .gates
                .get_mut(n.as_str())
                .expect("gate state is built from the same config");
            if g.chord.iter().all(|b| input.held.contains(b)) {
                st.held_ms += dt_ms;
                if st.held_ms >= g.hold_ms as f32 && !st.fired {
                    st.on = !st.on;
                    st.fired = true;
                }
            } else {
                st.held_ms = 0.0;
                st.fired = false;
            }
        }
    }

    pub fn tick(&mut self, input: &Inputs, dt_ms: f32) -> [u16; 16] {
        let c = self.config;

        /* handle link loss */

        if input.connected {
            self.disconnected_ms = 0.0;
        } else {
            self.disconnected_ms += dt_ms;
        }

        if self.disconnected_ms >= c.link.timeout_ms as f32 {
            for g in self.gates.values_mut() {
                g.on = false;
                g.held_ms = 0.0;
                g.fired = true;
            }
        } else {
            self.update_gates(input, dt_ms);
        }

        /* find currently enabled outputs */

        let enabled: BTreeMap<&str, bool> = c
            .outputs
            .iter()
            .map(|(n, o)| {
                let Output::Motor { enabled_by, .. } = o;
                let on = enabled_by
                    .iter()
                    .any(|g| self.gates.get(g.as_str()).is_some_and(|s| s.on));
                (n.as_str(), on)
            })
            .collect();

        /* signals */

        let mut signals: BTreeMap<&str, f32> = BTreeMap::new();
        for (n, i) in &c.inputs {
            let v = match i {
                Input::Axis { source, invert } => {
                    let raw = input.axes.get(source).copied().unwrap_or(0.0);
                    let raw = if raw.is_finite() {
                        raw.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    if *invert { -raw } else { raw }
                }
                Input::Button { source } => {
                    if input.held.contains(source) {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            signals.insert(n.as_str(), v);
        }

        /* behaviors */

        for (n, b) in &c.behaviors {
            let frozen = self
                .feeds
                .get(n.as_str())
                .is_some_and(|outs| outs.iter().all(|o| !enabled[o]));
            let state = self
                .behaviors
                .get_mut(n.as_str())
                .expect("behavior state is built from the same config");
            *state = if frozen {
                behavior_init(b)
            } else {
                advance_behavior(c, b, *state, input)
            };
            signals.insert(n.as_str(), *state);
        }

        /* drive mixing */

        for (name, us) in mix(c, &signals) {
            signals.insert(name, us);
        }

        /* output stage: gate, scale/invert, neutral, clamp, slew */

        let mut channels = [c.link.idle_us; 16];
        for (n, o) in &c.outputs {
            let Output::Motor {
                channel,
                source,
                scale,
                invert,
                range,
                slew,
                init,
                ..
            } = o;
            let (lo, hi) = (range.0 as f32, range.1 as f32);
            let cur = self
                .outputs
                .get_mut(n.as_str())
                .expect("output state is built from the same config");

            if enabled[n.as_str()] {
                let src = signals
                    .get(source.as_str())
                    .copied()
                    .unwrap_or(*init as f32);
                let sign = if *invert { -1.0 } else { 1.0 };
                let target = (NEUTRAL_US + (src - NEUTRAL_US) * scale * sign).clamp(lo, hi);
                *cur = match slew {
                    Slew::Unlimited => target,
                    Slew::PerSecond(rate) => {
                        let step = *rate as f32 * dt_ms / 1000.0;
                        *cur + (target - *cur).clamp(-step, step)
                    }
                };
            } else {
                *cur = *init as f32; // disabled; snap to the resting value, no ramp
            }
            channels[*channel as usize] = cur.round().clamp(lo, hi) as u16;
        }

        channels
    }
}
