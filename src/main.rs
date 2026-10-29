use std::time::{Duration, Instant};

use crsf::{Packet, PacketAddress, RcChannels};
use gilrs::{Axis, Button, Event, Gilrs};

// Convert standard RC PWM microseconds (1000us - 2000us)
// into CRSF 11bit representation (172 - 1811)
fn us_to_crsf(us: u16) -> u16 {
    let clamped_us = us.clamp(1000, 2000) as f32;
    (172.0 + ((clamped_us - 1000.0) / 1000.0) * 1639.0).round() as u16
}

const PORT_NAME: &str =
    "/dev/serial/by-id/usb-Silicon_Labs_CP2102_USB_to_UART_Bridge_Controller_0001-if00-port0";
const BAUD_RATE: u32 = 921600;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Opening serial port: {} at baud: {}...",
        PORT_NAME, BAUD_RATE
    );
    let mut serial = serialport::new(PORT_NAME, BAUD_RATE)
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
    let mut channels_us: [u16; 16] = [1500; 16];
    // If any esc or whatever require a startup value, define it here (for now)
    // example:
    // channels_us[2] = 1000;

    println!("\nCRSF Controller Streaming Begin...");
    println!("Press South to change channel 3");
    println!("Move Right Joystick to change channels 1 & 2");
    println!("Live CRSF Packets:\n");

    let mut last_send = Instant::now();

    enum WeaponIncrementDirection {
        UP,
        DOWN,
        ZERO,
    }

    enum InputType {
        Button {
            channel: usize,
            button: gilrs::Button,
            direction: WeaponIncrementDirection,
        },
        Joystick {
            channel: usize,
            axis: gilrs::Axis,
        },
    }

    let inputs: Vec<InputType> = vec![
        InputType::Button {
            channel: 2,
            button: Button::South,
            direction: WeaponIncrementDirection::UP,
        },
        /*
        InputType::Button {
            channel: 2,
            button: Button::North,
            direction: WeaponIncrementDirection::ZERO,
        },
        InputType::Button {
            channel: 2,
            button: Button::West,
            direction: WeaponIncrementDirection::DOWN,
        },
        */

        InputType::Joystick {
            channel: 0,
            axis: Axis::LeftStickY,
        },
        InputType::Joystick {
            channel: 1,
            axis: Axis::RightStickY,
            // axis: Axis::LeftStickX,
        },
    ];

    loop {
        while let Some(Event { id, event, .. }) = g.next_event() {
            active = Some(id);

            /*
            // handle button presses
            match event {
                gilrs::EventType::ButtonPressed(Button::South, _) => {
                    channels_us[2] += 100;
                    if channels_us[2] > 2000 {
                        channels_us[2] = 1000;
                    }
                }
                _ => {}
            }
            */

            if let gilrs::EventType::ButtonPressed(pressed, _) = event {
                for input in &inputs {
                    if let InputType::Button { channel, button, direction } = input {
                        if *button == pressed {
                            /*
                            match direction {
                                WeaponIncrementDirection::UP {

                                }
                            }
                            */
                            channels_us[*channel] += 100;
                            if channels_us[*channel] > 2000 {
                                // channels_us[*channel] = 1000;
                                channels_us[*channel] = 1500;
                            }
                        }
                    }
                }
            }
        }

        // handle continuous axis input
        if let Some(id) = active {
            let gamepad = g.gamepad(id);

            /*
            let ly = gamepad.value(Axis::LeftStickY);
            let ry = gamepad.value(Axis::RightStickY);

            channels_us[0] = (1500.0 + (ly * 500.0)).clamp(1000.0, 2000.0) as u16;
            channels_us[1] = (1500.0 + (ry * 500.0)).clamp(1000.0, 2000.0) as u16;
            */

            for input in &inputs {
                if let InputType::Joystick { channel, axis } = input {
                    let v = gamepad.value(*axis);
                    channels_us[*channel] = (1500.0 + (v * 500.0)).clamp(1000.0, 2000.0) as u16;
                }
            }
        }

        if last_send.elapsed() >= Duration::from_millis(20) {
            let mut crsf = [0u16; 16];
            for i in 0..16 {
                crsf[i] = us_to_crsf(channels_us[i]);
            }
            let packet = Packet::RcChannels(RcChannels(crsf));
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
