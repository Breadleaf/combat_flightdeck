use std::collections::{BTreeMap, BTreeSet};

use crate::config::{
    Behavior, CONFIG_VERSION, Config, Drive, Edge, Gain, Input, Output, Press, Reverse, SkidSource,
    Slew,
};

// names in the shared signal namespace
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Axis,
    Button,
    Behavior,
    DriveMotor,
}

fn finite(e: &mut Vec<String>, ctx: &str, v: f32) {
    if !v.is_finite() {
        e.push(format!("{ctx}: {v} is not a finite number"));
    }
}

fn behavior_values(b: &Behavior) -> Vec<f32> {
    match b {
        Behavior::Toggle { a, b, .. } => vec![*a, *b],
        Behavior::Hold { value, default, .. } => vec![*value, *default],
        Behavior::Select { options, init } => {
            let mut v: Vec<f32> = options.iter().map(|o| o.value).collect();
            v.push(*init);
            v
        }
        Behavior::Step {
            min,
            max,
            reset_to,
            init,
            ..
        } => vec![*min, *max, *init, *reset_to],
    }
}

pub fn validate(config: &Config) -> Result<(), Vec<String>> {
    let mut e: Vec<String> = Vec::new();

    /* Version */

    if config.version != CONFIG_VERSION {
        e.push(format!(
            "version {:?} does not match the version this program expects, {:?}",
            config.version, CONFIG_VERSION
        ));
    }

    /* Link */

    let l = &config.link;
    if l.baud == 0 {
        e.push("link: baud must be greater than 0".into());
    }
    if !(1..=1000).contains(&l.packet_rate_hz) {
        e.push(format!(
            "link: packet_rate_hz {} must between 1 and 1000",
            l.packet_rate_hz
        ));
    }
    if l.timeout_ms == 0 {
        e.push("link: timeout_ms must be greater than 0".into());
    }
    if !(1000..=2000).contains(&l.idle_us) {
        e.push(format!(
            "link: idle_us {} must be between 1000 and 2000",
            l.idle_us
        ));
    }

    /* Shared Signal Namespace (inputs, behaviors, drive motors) */

    let mut kinds: BTreeMap<&str, Kind> = BTreeMap::new();
    for (name, input) in &config.inputs {
        let k = match input {
            Input::Axis { .. } => Kind::Axis,
            Input::Button { .. } => Kind::Button,
        };
        kinds.insert(name, k);
    }
    for name in config.behaviors.keys() {
        if kinds.insert(name, Kind::Behavior).is_some() {
            e.push(format!(
                "name '{name}' is used by both an input and a behavior"
            ));
        }
    }
    let drive_motors: Vec<&str> = match &config.drive {
        Drive::Skid { left, right, .. } => left.iter().chain(right).map(|s| s.as_str()).collect(),
        Drive::Matrix { rows, .. } => rows.keys().map(|s| s.as_str()).collect(),
    };
    for name in &drive_motors {
        if kinds.insert(name, Kind::DriveMotor).is_some() {
            e.push(format!(
                "drive motor '{name}' is used by another input, behavior, or drive motor"
            ));
        }
    }

    let need = |e: &mut Vec<String>, ctx: &str, name: &str, ok: &[Kind]| match kinds.get(name) {
        None => e.push(format!("{ctx}: '{name}' does not exist")),
        Some(k) if !ok.contains(k) => e.push(format!(
            "{ctx}: '{name}' is a {k:?}, expected one of {ok:?}"
        )),
        Some(_) => {}
    };
    let press = |e: &mut Vec<String>, ctx: &str, p: &Press| {
        if let Press::On(n) = p {
            need(e, ctx, n, &[Kind::Button]);
        }
    };

    /* Behaviors */

    for (name, b) in &config.behaviors {
        let ctx = format!("behavior '{name}'");
        match b {
            Behavior::Toggle { on, a, b, init } => {
                need(&mut e, &ctx, on, &[Kind::Button]);
                finite(&mut e, &ctx, *a);
                finite(&mut e, &ctx, *b);
                if init != a && init != b {
                    e.push(format!("{ctx}: init {init} must equal a ({a}) or b ({b})"));
                }
            }
            Behavior::Hold { on, value, default } => {
                need(&mut e, &ctx, on, &[Kind::Button]);
                finite(&mut e, &ctx, *value);
                finite(&mut e, &ctx, *default);
            }
            Behavior::Select { options, init } => {
                if options.is_empty() {
                    e.push(format!("{ctx}: options is empty"));
                }
                for o in options {
                    need(&mut e, &ctx, &o.on, &[Kind::Button]);
                    finite(&mut e, &ctx, o.value);
                }
                finite(&mut e, &ctx, *init);
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
                init,
            } => {
                press(&mut e, &ctx, up);
                press(&mut e, &ctx, down);
                press(&mut e, &ctx, reset);
                if [up, down, reset].iter().all(|p| matches!(p, Press::Never)) {
                    e.push(format!(
                        "{ctx}: up, down, and reset are all Never, nothing can change it"
                    ));
                }
                for (what, v) in
                    [("by", by), ("min", min), ("max", max), ("reset_to", reset_to), ("init", init)]
                {
                    finite(&mut e, &format!("{ctx} {what}"), *v);
                }
                if *by <= 0.0 {
                    e.push(format!("{ctx}: by must be greater than 0.0"));
                }
                if min >= max {
                    e.push(format!("{ctx}: min {min} must be less than max {max}"));
                }
                let inside = |v: f32| v >= *min && v <= *max;
                if !inside(*init) {
                    e.push(format!("{ctx}: init {init} is outside min..max"));
                }
                if !inside(*reset_to) {
                    e.push(format!("{ctx}: reset_to {reset_to} is outside min..max"));
                }
                for (what, edge) in [("at_min", at_min), ("at_max", at_max)] {
                    if let Edge::Wrap(x) = edge {
                        if !inside(*x) {
                            e.push(format!("{ctx}: {what} Wrap({x}) is outside min..max"));
                        }
                    }
                }
            }
        }
    }

    /* Drive */

    let axis_or_behavior = [Kind::Axis, Kind::Behavior];
    match &config.drive {
        Drive::Skid {
            source,
            left,
            right,
            gain,
            reversed_when,
            ..
        } => {
            match source {
                SkidSource::Arcade { throttle, steer } => {
                    need(&mut e, "drive throttle", throttle, &axis_or_behavior);
                    need(&mut e, "drive steer", steer, &axis_or_behavior);
                }
                SkidSource::Tank { left, right } => {
                    need(&mut e, "drive tank left", left, &axis_or_behavior);
                    need(&mut e, "drive tank right", right, &axis_or_behavior);
                    if matches!(reversed_when, Reverse::When(_)) {
                        e.push("drive: reversed_when with a Tank source is not implemented yet; use Never".into());
                    }
                }
            }
            if left.is_empty() || right.is_empty() {
                e.push("drive: left and right must each list at least one motor".into());
            }
            match gain {
                Gain::Fixed(g) => {
                    finite(&mut e, "drive gain", *g);
                    if !(0.0..=1.0).contains(g) {
                        e.push(format!(
                            "drive gain: Fixed({g}) must be between 0.0 and 1.0"
                        ));
                    }
                }
                Gain::From(n) => need(&mut e, "drive gain", n, &[Kind::Behavior]),
            }
            if let Reverse::When(n) = reversed_when {
                need(&mut e, "drive reversed_when", n, &[Kind::Behavior]);
            }
        }
        Drive::Matrix { inputs, rows, .. } => {
            if inputs.is_empty() || rows.is_empty() {
                e.push("drive matrix: inputs and rows must not be empty".into());
            }
            for n in inputs {
                need(&mut e, "drive matrix input", n, &axis_or_behavior);
            }
            for (name, row) in rows {
                if row.len() != inputs.len() {
                    e.push(format!(
                        "drive matrix row '{name}' has {} values for {} inputs",
                        row.len(),
                        inputs.len()
                    ));
                }
                for c in row {
                    finite(&mut e, &format!("drive matrix row '{name}'"), *c);
                }
            }
        }
    }

    /* Gates */

    for (name, g) in &config.gates {
        let ctx = format!("gate '{name}'");
        if g.chord.is_empty() {
            e.push(format!("{ctx}: chord is empty"));
        }
        if g.hold_ms == 0 {
            e.push(format!("{ctx}: hold_ms must be greater than 0"));
        }
        for (i, b) in g.chord.iter().enumerate() {
            if g.chord[..i].contains(b) {
                e.push(format!("{ctx}: {b:?} is listed twice in the chord"));
            }
            let clash = config
                .inputs
                .iter()
                .find(|(_, inp)| matches!(inp, Input::Button { source } if source == b));
            if let Some((input_name, _)) = clash {
                e.push(format!(
                    "{ctx}: {b:?} is also input '{input_name}', so arming would trigger it"
                ));
            }
        }
    }
    let gate_names: Vec<&String> = config.gates.keys().collect();
    for (i, a) in gate_names.iter().enumerate() {
        for b in &gate_names[i + 1..] {
            let (ca, cb) = (&config.gates[*a].chord, &config.gates[*b].chord);
            if ca.iter().all(|x| cb.contains(x)) || cb.iter().all(|x| ca.contains(x)) {
                e.push(format!(
                    "gates '{a}' and '{b}': one chord contains the other, so holding one fires both"
                ));
            }
        }
    }

    /* Outputs */

    let mut channels: BTreeMap<u8, &str> = BTreeMap::new();
    let mut used_sources: BTreeSet<&str> = BTreeSet::new();
    for (name, o) in &config.outputs {
        let Output::Motor {
            channel,
            source,
            scale,
            range,
            slew,
            init,
            enabled_by,
            ..
        } = o;
        let ctx = format!("output '{name}'");

        if *channel > 15 {
            e.push(format!("{ctx}: channel {channel} must be between 0 and 15"));
        }
        if let Some(other) = channels.insert(*channel, name) {
            e.push(format!(
                "{ctx}: channel {channel} is also used by '{other}'"
            ));
        }
        need(&mut e, &ctx, source, &[Kind::Behavior, Kind::DriveMotor]);
        used_sources.insert(source);

        finite(&mut e, &ctx, *scale);
        if !(*scale > 0.0 && *scale <= 1.0) {
            e.push(format!(
                "{ctx}: scale {scale} must be greater than 0.0 and less than or equal to 1.0"
            ));
        }
        if range.0 >= range.1 || range.0 < 1000 || range.1 > 2000 {
            e.push(format!(
                "{ctx}: range {range:?} must be increasing and between 1000 and 2000"
            ));
        }
        if *init < range.0 || *init > range.1 {
            e.push(format!("{ctx}: init {init} is outside range {range:?}"));
        }
        if matches!(slew, Slew::PerSecond(0)) {
            e.push(format!("{ctx}: PerSecond(0) will never move"));
        }
        if enabled_by.is_empty() {
            e.push(format!(
                "{ctx}: enabled_by is empty, so it could never be enabled"
            ));
        }
        for g in enabled_by {
            if !config.gates.contains_key(g) {
                e.push(format!("{ctx}: gate '{g}' does not exist"));
            }
        }
        if let Some(b) = config.behaviors.get(source) {
            for v in behavior_values(b) {
                if v < range.0 as f32 || v > range.1 as f32 {
                    e.push(format!(
                        "{ctx}: behavior '{source}' can produce {v}, outside range {range:?}"
                    ));
                }
            }
        }
    }
    for m in &drive_motors {
        if !used_sources.contains(m) {
            e.push(format!(
                "drive motor '{m}' is not used as the source of any output"
            ));
        }
    }

    if e.is_empty() { Ok(()) } else { Err(e) }
}
