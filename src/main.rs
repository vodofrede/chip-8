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
const MEMORY_SIZE: usize = 4096; // 4KB
const REGISTER_COUNT: usize = 16;
const STACK_SIZE: usize = 16;
const START_ADDR: usize = 0x0200; // 0..0x0200 is reserved
const SCREEN_WIDTH: usize = 64; // pixels
const SCREEN_HEIGHT: usize = 32; // pixels
const SCALING_FACTOR: u32 = 16; // console pixel : real pixels
const FRAME_RATE: u32 = 60; // hz
const FRAME_TIME: Duration = Duration::new(0, 1_000_000_000 / FRAME_RATE);
const BACKGROUND_COLOR: Color = Color::RGB(153, 102, 1);
const PIXEL_COLOR: Color = Color::RGB(255, 204, 1);
const FONT_SPRITES: &[u8] = &[
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

fn main() {
    // initialize frontend
    let ctx = sdl2::init().unwrap();
    let video = ctx.video().unwrap();
    let window = video
        .window(
            "chip8",
            SCREEN_WIDTH as u32 * SCALING_FACTOR,
            SCREEN_HEIGHT as u32 * SCALING_FACTOR,
        )
        .opengl()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().build().unwrap();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(
            PixelFormatEnum::RGB24,
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
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

    // initialize core
    let mut mem = [0; MEMORY_SIZE];
    mem[..FONT_SPRITES.len()].copy_from_slice(FONT_SPRITES); // setup fonts in memory
    let mut v = [0u8; REGISTER_COUNT];
    let mut stack = vec![0; STACK_SIZE];
    let (mut ir, mut pc) = (0, START_ADDR as u16);
    let (mut dt, mut st) = (0, 0);
    let mut keypad = [false; 16];
    let mut screen = [false; SCREEN_WIDTH * SCREEN_HEIGHT];

    // load game
    let game = if let [_, file, ..] = env::args().collect::<Vec<_>>().as_slice() {
        fs::read(file).unwrap()
    } else {
        println!("Usage: chip8 <GAME_PATH>");
        return;
    };
    mem[START_ADDR..(START_ADDR + game.len())].copy_from_slice(&game);

    // run forever
    let mut time_last = Instant::now();
    let mut frame_time = 0;
    loop {
        // emulate a frame
        frame_time += FRAME_TIME.as_micros() as isize;
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
                            keypad[k] = true;
                        }
                    }
                    Event::KeyUp {
                        keycode: Some(keycode),
                        ..
                    } => {
                        if let Some(k) = button(keycode) {
                            keypad[k] = false;
                        }
                    }
                    _ => {}
                }
            }

            let hi = mem[pc as usize] as u16;
            let lo = mem[pc as usize + 1] as u16;
            let op = (hi << 8) | lo;
            pc += 2;

            let tick_time = match (
                (op & 0xF000) >> 12,
                (op & 0x0F00) >> 8,
                (op & 0x00F0) >> 4,
                op & 0x000F,
            ) {
                // cls
                (0, 0, 0xE, 0) => {
                    screen.fill(false);
                    109
                }
                // ret
                (0, 0, 0xE, 0xE) => {
                    pc = stack.pop().unwrap();
                    105
                }
                // jp
                (1, ..) => {
                    pc = op & 0x0FFF;
                    105
                }
                // call
                (2, ..) => {
                    stack.push(pc);
                    pc = op & 0x0FFF;
                    105
                }
                // se vx nn
                (3, x, ..) => {
                    if v[x as usize] == nn(op) {
                        pc += 2
                    }
                    55
                }
                // sne vx nn
                (4, x, ..) => {
                    if v[x as usize] != nn(op) {
                        pc += 2
                    }
                    55
                }
                // se vx vy
                (5, x, y, _) => {
                    if v[x as usize] == v[y as usize] {
                        pc += 2
                    }
                    73
                }
                // ld vx nn
                (6, x, ..) => {
                    v[x as usize] = nn(op);
                    27
                }
                // add vx byte
                (7, x, ..) => {
                    v[x as usize] = v[x as usize].wrapping_add(nn(op));
                    45
                }
                // set vx vy
                (8, x, y, 0) => {
                    v[x as usize] = v[y as usize];
                    200
                }
                // or vx vy
                (8, x, y, 1) => {
                    v[x as usize] |= v[y as usize];
                    v[0xF] = 0;
                    200
                }
                // and vx vy
                (8, x, y, 2) => {
                    v[x as usize] &= v[y as usize];
                    v[0xF] = 0;
                    200
                }
                // xor vx vy
                (8, x, y, 3) => {
                    v[x as usize] ^= v[y as usize];
                    v[0xF] = 0;
                    200
                }
                // add vx vy
                (8, x, y, 4) => {
                    let (res, overflow) = v[x as usize].overflowing_add(v[y as usize]);
                    v[x as usize] = res;
                    v[0xF] = overflow as u8;
                    200
                }
                // sub vx vy
                (8, x, y, 5) => {
                    let (res, borrow) = v[x as usize].overflowing_sub(v[y as usize]);
                    v[x as usize] = res;
                    v[0xF] = !borrow as u8;
                    200
                }
                // shr vx
                (8, x, y, 6) => {
                    v[x as usize] = v[y as usize];
                    let lsb = v[x as usize] & 1;
                    v[x as usize] >>= 1;
                    v[0xF] = lsb;
                    200
                }
                // sub vx vy
                (8, x, y, 7) => {
                    let (res, borrow) = v[y as usize].overflowing_sub(v[x as usize]);
                    v[x as usize] = res;
                    v[0xF] = !borrow as u8;
                    200
                }
                // shl vx
                (8, x, y, 0xE) => {
                    v[x as usize] = v[y as usize];
                    let msb = (v[x as usize] >> 7) & 1;
                    v[x as usize] <<= 1;
                    v[0xF] = msb;
                    200
                }
                // sne vx, vy
                (9, x, y, 0) => {
                    if v[x as usize] != v[y as usize] {
                        pc += 2
                    }
                    73
                }
                // ld i nnn
                (0xA, ..) => {
                    ir = op & 0x0FFF;
                    55
                }
                // jp v0 nnn
                (0xB, ..) => {
                    pc = v[0] as u16 + nnn(op);
                    105
                }
                // rnd vx nn
                (0xC, x, ..) => {
                    v[x as usize] = rand::random::<u8>() & nn(op);
                    164
                }
                // drw vx vy n
                (0xD, x, y, n) => {
                    let x_coord = v[x as usize] as u16;
                    let y_coord = v[y as usize] as u16;

                    let mut flipped = false;
                    for y_line in 0..n {
                        let addr = ir + y_line;
                        let pixels = mem[addr as usize];
                        for x_line in 0..8 {
                            if (pixels & (0b1000_0000 >> x_line)) != 0 {
                                let x = (x_coord + x_line) as usize % SCREEN_WIDTH;
                                let y = (y_coord + y_line) as usize % SCREEN_HEIGHT;
                                let idx = x + SCREEN_WIDTH * y;
                                flipped |= screen[idx];
                                screen[idx] ^= true;
                            }
                        }
                    }
                    v[0xF] = flipped as u8;

                    22734
                }
                // skp vx
                (0xE, x, 9, 0xE) => {
                    if keypad[v[x as usize] as usize] {
                        pc += 2;
                    }
                    73
                }
                // sknp vx
                (0xE, x, 0xA, 1) => {
                    if !keypad[v[x as usize] as usize] {
                        pc += 2;
                    }
                    73
                }
                // ld vx dt
                (0xF, x, 0, 7) => {
                    v[x as usize] = dt;
                    45
                }
                // ld vx k
                (0xF, x, 0, 0xA) => {
                    let mut pressed = false;
                    #[allow(clippy::needless_range_loop)]
                    for i in 0..keypad.len() {
                        if keypad[i] {
                            pressed = true;
                            v[x as usize] = i as u8;
                            // wait for release

                            break;
                        }
                    }
                    if !pressed {
                        pc -= 2;
                    }
                    100
                }
                // ld dt vx
                (0xF, x, 1, 5) => {
                    dt = v[x as usize];
                    45
                }
                // ld st vx
                (0xF, x, 1, 8) => {
                    st = v[x as usize];
                    45
                }
                // add ir vx
                (0xF, x, 1, 0xE) => {
                    ir = ir.wrapping_add(v[x as usize] as u16);
                    86
                }
                // ld f vx
                (0xF, x, 2, 9) => {
                    ir = v[x as usize] as u16 * 5;
                    91
                }
                // ld b cx
                (0xF, x, 3, 3) => {
                    let vx = v[x as usize];
                    mem[ir as usize] = (vx / 100) % 10;
                    mem[ir as usize + 1] = (vx / 10) % 10;
                    mem[ir as usize + 2] = vx % 10;
                    927
                }
                // ld ir vx
                (0xF, x, 5, 5) => {
                    for offset in 0..=(x as usize) {
                        mem[ir as usize + offset] = v[offset];
                    }
                    ir += 1;
                    605
                }
                // ld vx ir
                (0xF, x, 6, 5) => {
                    for offset in 0..=(x as usize) {
                        v[offset] = mem[ir as usize + offset];
                    }
                    ir += 1;
                    605
                }
                _ => todo!("unimplemented opcode: {op:04x}"),
            };

            frame_time -= tick_time;
        }
        dt = dt.saturating_sub(1);
        st = st.saturating_sub(1);
        if st > 0 {
            device.resume()
        } else {
            device.pause()
        }

        // present the frame buffer
        // draw on the texture
        let _ = texture.with_lock(None, |pixels: &mut [u8], pitch: usize| {
            for i in (0..(pitch * SCREEN_HEIGHT)).step_by(3) {
                // fade existing pixels to black to simulate display fading
                pixels[i] = lerp(pixels[i], BACKGROUND_COLOR.r, 0.3, 5);
                pixels[i + 1] = lerp(pixels[i + 1], BACKGROUND_COLOR.g, 0.3, 5);
                pixels[i + 2] = lerp(pixels[i + 2], BACKGROUND_COLOR.b, 0.3, 5);

                // draw new pixels
                if screen[i / 3] {
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

const fn nn(op: u16) -> u8 {
    (op & 0x00FF) as u8
}
const fn nnn(op: u16) -> u16 {
    op & 0x0FFF
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
