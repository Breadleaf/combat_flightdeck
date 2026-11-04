use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) version: Version,
    pub(crate) link: Link,
    pub(crate) inputs: BTreeMap<String, Input>,
    pub(crate) behaviors: BTreeMap<String, Behavior>,
    pub(crate) drive: Drive,
    pub(crate) gates: BTreeMap<String, Gate>,
    pub(crate) outputs: BTreeMap<String, Output>,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Version(u8, u8, u8);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Link {
    pub(crate) port: String,
    pub(crate) baud: u32,
    pub(crate) packet_rate_hz: u32,
    pub(crate) timeout_ms: u32,
    pub(crate) idle_us: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Input {
    // https://docs.rs/gilrs/latest/gilrs/ev/enum.Axis.html
    Axis { source: gilrs::Axis, invert: bool },
    // https://docs.rs/gilrs/latest/gilrs/ev/enum.Button.html
    Button { source: gilrs::Button },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Behavior {
    Toggle {
        on: String,
        a: f32,
        b: f32,
        init: f32,
    },
    Hold {
        on: String,
        value: f32,
        default: f32,
    },
    Select {
        options: Vec<SelectOption>,
        init: f32,
    },
    Step {
        up: Press,
        down: Press,
        reset: Press,
        by: f32,
        min: f32,
        max: f32,
        at_min: Edge,
        at_max: Edge,
        reset_to: f32,
        init: f32,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectOption {
    on: String,
    value: f32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Press {
    Never,
    On(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Edge {
    Wrap(f32),
    Clamp,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Drive {
    Skid {
        source: SkidSource,
        left: Vec<String>,
        right: Vec<String>,
        gain: Gain,
        reversed_when: Reverse,
        normalize: bool,
    },
    Matrix {
        inputs: Vec<String>,
        rows: BTreeMap<String, Vec<f32>>,
        normalize: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum SkidSource {
    Arcade { throttle: String, steer: String },
    Tank { left: String, right: String },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Gain {
    Fixed(f32),
    From(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Reverse {
    Never,
    When(String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Gate {
    pub(crate) chord: Vec<gilrs::Button>,
    pub(crate) hold_ms: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Output {
    Motor {
        channel: u8,
        source: String,
        scale: f32,
        invert: bool,
        range: (u16, u16),
        slew: Slew,
        init: u16,
        enabled_by: Vec<String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Slew {
    Unlimited,
    PerSecond(u32), // µs
}

pub(crate) const CONFIG_VERSION: Version = Version(1, 2, 3);

pub(crate) fn load_config(path: &str) -> Result<Config, String> {
    let raw_text = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    ron::from_str::<Config>(&raw_text).map_err(|e| format!("cannot parse {path}: {e}"))
}
