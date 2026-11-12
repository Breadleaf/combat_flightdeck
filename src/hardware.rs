use std::time::{Duration, Instant};

use crsf::{Packet, PacketAddress, RcChannels};
use gilrs::{Event, EventType, Gilrs};

use crate::{
    config::Config,
    runtime::{Inputs, Runtime},
};

fn us_to_crsf(us: u16) -> u16 {
    let clamped_us = us.clamp(1000, 2000) as f32;
    (172.0 + ((clamped_us - 1000.0) / 1000.0) * 1639.0).round() as u16
}

pub fn run(config: &Config, mut runtime: Runtime) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Opening Serial Port: {} at {} baud...",
        config.link.port, config.link.baud
    );
    let mut serial = serialport::new(&config.link.port, config.link.baud)
        .timeout(Duration::from_millis(10))
        .open()?;

    let mut g = Gilrs::new().map_err(|e| format!("cannot start gamepad support: {e}"))?;
    let pad = g
        .gamepads()
        .next()
        .map(|(id, _)| id)
        .ok_or("no gamepad found, connect one and try again")?;

    let buttons = runtime.watched_buttons();
    let axes = runtime.watched_axis();

    let mut inputs = Inputs::default();
    let period = Duration::from_secs_f64(1.0 / config.link.packet_rate_hz as f64);
    let mut next = Instant::now();
    let mut last = Instant::now();
    let mut announced: Vec<String> = Vec::new();

    loop {
        while let Some(Event { id, event, .. }) = g.next_event() {
            if id == pad {
                if let EventType::ButtonPressed(button, _) = event {
                    inputs.pressed.insert(button);
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

            let gamepad = g.gamepad(pad);
            inputs.connected = gamepad.is_connected();
            inputs.held.clear();
            inputs.axes.clear();
            if inputs.connected {
                for b in &buttons {
                    if gamepad.is_pressed(*b) {
                        inputs.held.insert(*b);
                    }
                }
                for a in &axes {
                    inputs.axes.insert(*a, gamepad.value(*a));
                }
            }

            let channels = runtime.tick(&inputs, dt_ms);
            inputs.pressed.clear();

            let mut crsf = [0u16; 16];
            for (i, us) in channels.iter().enumerate() {
                crsf[i] = us_to_crsf(*us);
            }
            let packet = Packet::RcChannels(RcChannels(crsf));
            let raw_packet = packet.into_raw(PacketAddress::Transmitter);
            if let Err(e) = serial.write_all(raw_packet.data()) {
                eprintln!("failed to write to serial port: {e}");
            }

            let on = runtime.gates_on();
            if on != announced {
                println!("gates on: {on:?}");
                announced = on;
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
