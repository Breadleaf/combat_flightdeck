use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::config::{self, Config, Output};

pub(crate) const SETUP_VERSION: (u8, u8, u8) = (0, 1, 0);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Setup {
    pub(crate) robots: BTreeMap<String, RobotEntry>,
    pub(crate) version: (u8, u8, u8),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename = "Robot")]
pub(crate) struct RobotEntry {
    pub(crate) controller: String,
    pub(crate) config: String,
}

pub(crate) struct LoadedRobot {
    pub(crate) name: String,
    pub(crate) controller: String,
    pub(crate) config: Config,
}

pub(crate) fn load_all(setup_path: &str) -> Result<Vec<LoadedRobot>, String> {
    let raw =
        fs::read_to_string(setup_path).map_err(|e| format!("cannot read {setup_path}: {e}"))?;
    let setup: Setup =
        ron::from_str(&raw).map_err(|e| format!("cannot parse {setup_path}: {e}"))?;

    if setup.version != SETUP_VERSION {
        return Err(format!(
            "setup version {:?} does not match the version this program expects, {:?}",
            setup.version, SETUP_VERSION
        ));
    }
    if setup.robots.is_empty() {
        return Err("setup: robots is empty".into());
    }

    let base = Path::new(setup_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let mut out = Vec::new();
    for (name, r) in setup.robots {
        let cfg_path = base.join(&r.config);
        let cfg_str = cfg_path
            .to_str()
            .ok_or_else(|| format!("robot '{name}': config path is not valid UTF-8"))?;
        let config = config::load_config(cfg_str).map_err(|e| format!("robot '{name}': {e}"))?;
        out.push(LoadedRobot {
            name,
            controller: r.controller,
            config,
        });
    }
    Ok(out)
}

pub(crate) fn validate_event(robots: &[LoadedRobot]) -> Result<(), Vec<String>> {
    let mut e: Vec<String> = Vec::new();

    let first = &robots[0];
    let fl = &first.config.link;
    for r in &robots[1..] {
        let l = &r.config.link;
        let mut diffs: Vec<String> = Vec::new();
        if l.port != fl.port {
            diffs.push(format!("port '{}' vs '{}'", l.port, fl.port));
        }
        if l.baud != fl.baud {
            diffs.push(format!("baud {} vs {}", l.baud, fl.baud));
        }
        if l.packet_rate_hz != fl.packet_rate_hz {
            diffs.push(format!(
                "packet_rate_hz {} vs {}",
                l.packet_rate_hz, fl.packet_rate_hz
            ));
        }
        if l.idle_us != fl.idle_us {
            diffs.push(format!("idle_us {} vs {}", l.idle_us, fl.idle_us));
        }
        if !diffs.is_empty() {
            e.push(format!(
                "robot '{}' link differs from robot '{}': {} (timeout_ms may differ)",
                r.name,
                first.name,
                diffs.join(", ")
            ));
        }
    }

    let mut owner: BTreeMap<u8, (&str, &str)> = BTreeMap::new();
    for r in robots {
        for (out_name, o) in &r.config.outputs {
            let Output::Motor { channel, .. } = o;
            if let Some((other_robot, other_out)) =
                owner.insert(*channel, (r.name.as_str(), out_name.as_str()))
            {
                if other_robot != r.name {
                    e.push(format!("channel {channel} is used by robot '{other_robot}' output '{other_out}' and robot '{}' output '{out_name}'", r.name));
                }
            }
        }
    }

    if e.is_empty() { Ok(()) } else { Err(e) }
}
