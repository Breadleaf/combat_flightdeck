use std::{collections::BTreeMap, env, fs, process};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: Version,
    link: Link,
    inputs: BTreeMap<String, Input>,
    behaviors: BTreeMap<String, Behavior>,
    drive: Drive,
    gates: BTreeMap<String, Gate>,
    outputs: BTreeMap<String, Output>,
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Version(u8, u8, u8);

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Link {
    port: String,
    baud: u32,
    packet_rate_hz: u32,
    timeout_ms: u32,
    idle_us: u16,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Input {
    // https://docs.rs/gilrs/latest/gilrs/ev/enum.Axis.html
    Axis { source: gilrs::Axis, invert: bool },
    // https://docs.rs/gilrs/latest/gilrs/ev/enum.Button.html
    Button { source: gilrs::Button },
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Behavior {
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SelectOption {
    on: String,
    value: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Press {
    Never,
    On(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Edge {
    Wrap(f32),
    Clamp,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Drive {
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum SkidSource {
    Arcade { throttle: String, steer: String },
    Tank { left: String, right: String },
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Gain {
    Fixed(f32),
    From(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Reverse {
    Never,
    When(String),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Gate {
    chord: Vec<gilrs::Button>,
    hold_ms: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Output {
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
enum Slew {
    Unlimited,
    PerSecond(u32), // µs
}

const CONFIG_VERSION: Version = Version(1, 2, 3);

fn load_config(path: &str) -> Result<Config, String> {
    let raw_text = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    ron::from_str::<Config>(&raw_text).map_err(|e| format!("cannot parse {path}: {e}"))
}

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "example.ron".to_string());
    let config = match load_config(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };
    println!("{:#?}", config);
}
