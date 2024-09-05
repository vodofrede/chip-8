mod chip8;

use crate::chip8::Chip8;
use sdl2::{
    audio::{AudioCallback, AudioSpecDesired},
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
};
use std::{
    env, fs, thread,
    time::{Duration, Instant},
};

// console constants
const SCALING_FACTOR: u32 = 16; // console pixel : real pixels
const BACKGROUND_COLOR: Color = Color::RGB(153, 102, 1);
const PIXEL_COLOR: Color = Color::RGB(255, 204, 1);
const FRAME_RATE: u32 = 60; // hz
const FRAME_TIME: Duration = Duration::new(0, 1_000_000_000 / FRAME_RATE);

fn main() {
    // initialize core
    let mut chip8 = Chip8::new();
    let game = if let [_, file, ..] = env::args().collect::<Vec<_>>().as_slice() {
        fs::read(file).unwrap()
    } else {
        println!("Usage: chip8 <GAME_PATH>");
        return;
    };
    chip8.load(&game);
    let (screen_width, screen_height) = chip8.dimensions();

    // initialize frontend
    let ctx = sdl2::init().unwrap();
    let video = ctx.video().unwrap();
    let window = video
        .window(
            "chip8",
            screen_width as u32 * SCALING_FACTOR,
            screen_height as u32 * SCALING_FACTOR,
        )
        .opengl()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().build().unwrap();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(
            PixelFormatEnum::RGB24,
            screen_width as u32,
            screen_height as u32,
        )
        .unwrap();

    let audio = ctx.audio().unwrap();
    let spec = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(1),
        samples: None,
    };
    let device = audio
        .open_playback(None, &spec, |spec| SquareWave {
            phase_inc: 110.0 / spec.freq as f32,
            phase: 0.0,
            volume: 0.10,
        })
        .unwrap();

    let mut event_pump = ctx.event_pump().unwrap();

    // run forever
    let mut time_last = Instant::now();
    let mut frame_time = 0;
    loop {
        // emulate a frame
        frame_time += FRAME_TIME.as_micros() as i64;
        while frame_time > 0 {
            // get new input
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => return,
                    Event::KeyDown {
                        keycode: Some(keycode),
                        ..
                    } => {
                        if let Some(k) = button(keycode) {
                            chip8.keypad[k] = true;
                        }
                    }
                    Event::KeyUp {
                        keycode: Some(keycode),
                        ..
                    } => {
                        if let Some(k) = button(keycode) {
                            chip8.keypad[k] = false;
                        }
                    }
                    _ => {}
                }
            }

            // tick core
            let tick_time = chip8.tick();

            frame_time -= tick_time;
        }

        // advance timers and maybe play tone
        chip8.timers();
        if chip8.tone() {
            device.resume()
        } else {
            device.pause()
        }

        // present the frame buffer
        // draw on the texture
        let _ = texture.with_lock(None, |pixels: &mut [u8], pitch: usize| {
            for i in (0..(pitch * screen_height)).step_by(3) {
                // fade existing pixels to black to simulate display fading
                pixels[i] = lerp(pixels[i], BACKGROUND_COLOR.r, 0.3, 5);
                pixels[i + 1] = lerp(pixels[i + 1], BACKGROUND_COLOR.g, 0.3, 5);
                pixels[i + 2] = lerp(pixels[i + 2], BACKGROUND_COLOR.b, 0.3, 5);

                // draw new pixels
                if chip8.screen[i / 3] {
                    pixels[i] = PIXEL_COLOR.r;
                    pixels[i + 1] = PIXEL_COLOR.g;
                    pixels[i + 2] = PIXEL_COLOR.b;
                }
            }
        });

        // present the texture
        canvas.set_draw_color(BACKGROUND_COLOR);
        canvas.clear();
        canvas.copy(&texture, None, None).unwrap();
        canvas.present();

        // wait until next frame
        let time_now = Instant::now();
        let until_next_frame =
            FRAME_TIME.saturating_sub(time_now.saturating_duration_since(time_last));
        thread::sleep(until_next_frame);
        time_last = time_now;
    }
}

fn button(keycode: Keycode) -> Option<usize> {
    let index = match keycode {
        Keycode::Num1 => 0x1,
        Keycode::Num2 => 0x2,
        Keycode::Num3 => 0x3,
        Keycode::Num4 => 0xC,
        Keycode::Q => 0x4,
        Keycode::W => 0x5,
        Keycode::E => 0x6,
        Keycode::R => 0xD,
        Keycode::A => 0x7,
        Keycode::S => 0x8,
        Keycode::D => 0x9,
        Keycode::F => 0xE,
        Keycode::Z => 0xA,
        Keycode::X => 0x0,
        Keycode::C => 0xB,
        Keycode::V => 0xF,
        _ => return None,
    };
    Some(index)
}

struct SquareWave {
    phase_inc: f32,
    phase: f32,
    volume: f32,
}
impl AudioCallback for SquareWave {
    type Channel = f32;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        for x in out.iter_mut() {
            *x = if self.phase <= 0.5 {
                self.volume
            } else {
                -self.volume
            };
            self.phase = (self.phase + self.phase_inc) % 1.0;
        }
    }
}

fn lerp(start: u8, end: u8, t: f32, min: u8) -> u8 {
    if start.abs_diff(end) < min {
        end
    } else {
        (start as f32 + (end as f32 - start as f32) * t) as u8
    }
}
