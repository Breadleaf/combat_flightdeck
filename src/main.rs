use std::time::{Duration, Instant};

use crsf::{Packet, PacketAddress, RcChannels};
use gilrs::{Axis, Button, Event, Gilrs};
use serialport::SerialPort;

// Convert standard RC PWM microseconds (1000us - 2000us)
// into CRSF 11bit representation (172 - 1811)
fn us_to_crsf(us: u16) -> u16 {
    let clamped_us = us.clamp(1000, 2000) as f32;
    (172.0 + ((clamped_us - 1000.0) / 1000.0) * 1639.0).round() as u16
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port_name = "/dev/ttyUSB0";
    let baud_rate = 921600;

    println!(
        "Opening serial port: {} at baud: {}...",
        port_name, baud_rate
    );
    let mut serial = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(10))
        .open()?;

    let mut g = Gilrs::new().unwrap();
    let mut active = None;

    // get first connected gamepad
    for (id, gamepad) in g.gamepads() {
        println!("Connected Gamepad: {} ({:?})", gamepad.name(), id);
        active = Some(id);
        break;
    }

    if active.is_none() {
        return Err(String::from("No gamepads found, please connect and try again...").into());
    }

    // set 16 channels to midpoint
    let mut channels: [u16; 16] = [1500; 16];
    // set throttle pins to minimum for safety
    channels[2] = 1000;

    println!("\nCRSF Controller Streaming Begin...");
    println!("Press South to change channel 4");
    println!("Move Right Joystick to change channels 1 & 2");
    println!("Live CRSF Packets:\n");

    let mut last_send = Instant::now();

    loop {
        while let Some(Event { id, event, .. }) = g.next_event() {
            active = Some(id);

            // handle button presses
            match event {
                gilrs::EventType::ButtonPressed(Button::South, _) => {
                    channels[3] += 100;
                    if channels[3] > 2000 {
                        channels[3] = 1000;
                    }
                }
                _ => {}
            }
        }

        // handle continuous axis input
        if let Some(id) = active {
            let gamepad = g.gamepad(id);

            let rx = gamepad.value(Axis::RightStickX);
            let ry = gamepad.value(Axis::RightStickY);

            let us_ch1 = (1500.0 + (rx * 500.0)).clamp(1000.0, 2000.0) as u16;
            let us_ch2 = (1500.0 + (ry * 500.0)).clamp(1000.0, 2000.0) as u16;

            channels[0] = us_to_crsf(us_ch1);
            channels[1] = us_to_crsf(us_ch2);
        }

        if last_send.elapsed() >= Duration::from_millis(20) {
            let packet = Packet::RcChannels(RcChannels(channels));
            let raw_packet = packet.into_raw(PacketAddress::Transmitter);
            if let Err(e) = serial.write_all(raw_packet.data()) {
                eprintln!("Failed to write to serial port: {}", e);
            }
            println!("{:?}", raw_packet.data());
            last_send = Instant::now();
        }

        // small sleep so my CPU doesn't explode
        std::thread::sleep(Duration::from_millis(2));
    }
}
