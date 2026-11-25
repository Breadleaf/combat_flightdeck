use std::time::{Duration, Instant};

use crsf::{Packet, PacketAddress, RcChannels};

#[cfg(target_os = "linux")]
use gilrs::LinuxGamepadExt;
use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};

use crate::{
    config::Link,
    runtime::{Inputs, Runtime},
};

pub struct Entry<'a> {
    pub name: &'a str,
    pub controller: &'a str,
    pub runtime: Runtime<'a>,
}

struct Robot<'a> {
    name: &'a str,
    pad: GamepadId,
    runtime: Runtime<'a>,
    inputs: Inputs,
    buttons: Vec<Button>,
    axes: Vec<Axis>,
    channels: Vec<usize>, // the crsf channels this robot owns
    announced: Vec<String>,
}

fn us_to_crsf(us: u16) -> u16 {
    let clamped_us = us.clamp(1000, 2000) as f32;
    (172.0 + ((clamped_us - 1000.0) / 1000.0) * 1639.0).round() as u16
}

#[cfg(target_os = "linux")]
pub fn list_gamepads() -> Result<(), Box<dyn std::error::Error>> {
    let g = Gilrs::new().map_err(|e| format!("cannot start gamepad support: {e}"))?;
    for (_, gp) in g.gamepads() {
        println!("{} {}", gp.devpath().display(), gp.name());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn list_gamepads() -> Result<(), Box<dyn std::error::Error>> {
    Err("--list needs Linux (device paths are only available there)".into())
}

#[cfg(target_os = "linux")]
fn pick_pad(g: &Gilrs, want: &str) -> Result<GamepadId, String> {
    let want = std::fs::canonicalize(want)
        .map_err(|e| format!("cannot resolve controller path {want}: {e}"))?;
    g.gamepads()
        .find(|(_, gp)| gp.devpath() == want.as_path())
        .map(|id, _| id)
        .ok_or_else(|| {
            format!(
                "no gamepad as {} (run with --list to see what is connected)",
                want.display()
            )
        })
}

#[cfg(not(target_os = "linux"))]
fn pick_pad(_g: &Gilrs, _want: &str) -> Result<GamepadId, String> {
    Err(
        "selecting a controller by device path needs Linux (gilrs only exposes devpath there)"
            .into(),
    )
}

pub fn run(link: &Link, entries: Vec<Entry<'_>>) -> Result<(), Box<dyn std::error::Error>> {
    /* bind each robot to its pad before touching the radio; refuse to start half-ready */

    let mut g = Gilrs::new().map_err(|e| format!("cannot start gamepad support: {e}"))?;
    let mut robots: Vec<Robot> = Vec::new();
    for e in entries {
        let pad = pick_pad(&g, e.controller).map_err(|m| format!("robot '{}': {m}", e.name))?;
        if let Some(other) = robots.iter().find(|r| r.pad == pad) {
            return Err(format!(
                "robots '{}' and '{}' are both using controller {}",
                other.name, e.name, e.controller
            )
            .into());
        }
        println!("robot '{}' -> {}", e.name, e.controller);
        robots.push(Robot {
            name: e.name,
            pad,
            buttons: e.runtime.watched_buttons(),
            axes: e.runtime.watched_axis(),
            channels: e.runtime.used_channels(),
            runtime: e.runtime,
            inputs: Inputs::default(),
            announced: Vec::new(),
        });
    }

    println!(
        "Opening Serial Port: {} at {} baud...",
        link.port, link.baud
    );
    let mut serial = serialport::new(&link.port, link.baud)
        .timeout(Duration::from_millis(10))
        .open()?;

    let idle = us_to_crsf(link.idle_us);
    let period = Duration::from_secs_f64(1.0 / link.packet_rate_hz as f64);
    let mut next = Instant::now();
    let mut last = Instant::now();

    loop {
        while let Some(Event { id, event, .. }) = g.next_event() {
            if let EventType::ButtonPressed(button, _) = event {
                if let Some(r) = robots.iter_mut().find(|r| r.pad == id) {
                    r.inputs.pressed.insert(button);
                }
            }
        }

        let now = Instant::now();
        if now >= next {
            let dt_ms = now.duration_since(last).as_secs_f32() * 1000.0;
            last = now;
            next += period;
            if next < now {
                next = now + period // fell behind, dont try to catch up
            }

            let mut crsf = [idle; 16];

            for r in &mut robots {
                let gamepad = g.gamepad(r.pad);
                r.inputs.connected = gamepad.is_connected();
                r.inputs.held.clear();
                r.inputs.axes.clear();
                if r.inputs.connected {
                    for b in &r.buttons {
                        if gamepad.is_pressed(*b) {
                            r.inputs.held.insert(*b);
                        }
                    }
                    for a in &r.axes {
                        r.inputs.axes.insert(*a, gamepad.value(*a));
                    }
                }

                let channels = r.runtime.tick(&r.inputs, dt_ms);
                r.inputs.pressed.clear();

                for &i in &r.channels {
                    crsf[i] = us_to_crsf(channels[i]);
                }

                let on = r.runtime.gates_on();
                if on != r.announced {
                    println!("[{}] gates on: {on:?}", r.name);
                    r.announced = on;
                }
            }

            let packet = Packet::RcChannels(RcChannels(crsf));
            let raw_packet = packet.into_raw(PacketAddress::Transmitter);
            if let Err(e) = serial.write_all(raw_packet.data()) {
                eprintln!("failed to write to serial port: {e}");
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
