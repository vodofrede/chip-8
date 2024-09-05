// console constants
const MEMORY_SIZE: usize = 4096; // 4KB
const REGISTER_COUNT: usize = 16;
const STACK_SIZE: usize = 16;
const START_ADDR: usize = 0x0200; // 0..0x0200 is reserved
const SCREEN_WIDTH: usize = 64; // pixels
const SCREEN_HEIGHT: usize = 32; // pixels
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

pub struct Chip8 {
    mem: [u8; MEMORY_SIZE],
    v: [u8; REGISTER_COUNT],
    ir: u16,
    pc: u16,
    dt: u8,
    st: u8,
    stack: Vec<u16>,
    pub screen: [bool; SCREEN_WIDTH * SCREEN_HEIGHT],
    pub keypad: [bool; 16],
}
impl Chip8 {
    pub fn new() -> Self {
        let mut chip8 = Self {
            mem: [0; MEMORY_SIZE],
            v: [0u8; REGISTER_COUNT],
            stack: vec![0; STACK_SIZE],
            keypad: [false; 16],
            screen: [false; SCREEN_WIDTH * SCREEN_HEIGHT],
            ir: 0,
            pc: START_ADDR as u16,
            dt: 0,
            st: 0,
        };
        chip8.mem[..FONT_SPRITES.len()].copy_from_slice(FONT_SPRITES); // setup fonts in memory
        chip8
    }
    pub fn load(&mut self, game: &[u8]) {
        self.mem[START_ADDR..(START_ADDR + game.len())].copy_from_slice(game);
    }
    pub fn tick(&mut self) -> i64 {
        let op = self.fetch();

        self.execute(op)
    }
    pub fn timers(&mut self) {
        self.dt = self.dt.saturating_sub(1);
        self.st = self.st.saturating_sub(1);
    }
    pub fn tone(&self) -> bool {
        self.st > 0
    }
    pub fn dimensions(&self) -> (usize, usize) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    fn fetch(&mut self) -> u16 {
        let hi = self.mem[self.pc as usize] as u16;
        let lo = self.mem[self.pc as usize + 1] as u16;
        let op = (hi << 8) | lo;
        self.pc += 2;
        op
    }
    fn execute(&mut self, op: u16) -> i64 {
        // split op into 4 nibbles
        match (
            (op & 0xF000) >> 12,
            (op & 0x0F00) >> 8,
            (op & 0x00F0) >> 4,
            op & 0x000F,
        ) {
            // cls
            (0, 0, 0xE, 0) => {
                self.screen.fill(false);
                109
            }
            // ret
            (0, 0, 0xE, 0xE) => {
                self.pc = self.stack.pop().unwrap();
                105
            }
            // jp
            (1, ..) => {
                self.pc = op & 0x0FFF;
                105
            }
            // call
            (2, ..) => {
                self.stack.push(self.pc);
                self.pc = op & 0x0FFF;
                105
            }
            // se vx nn
            (3, x, ..) => {
                if self.v[x as usize] == nn(op) {
                    self.pc += 2
                }
                55
            }
            // sne vx nn
            (4, x, ..) => {
                if self.v[x as usize] != nn(op) {
                    self.pc += 2
                }
                55
            }
            // se vx vy
            (5, x, y, _) => {
                if self.v[x as usize] == self.v[y as usize] {
                    self.pc += 2
                }
                73
            }
            // ld vx nn
            (6, x, ..) => {
                self.v[x as usize] = nn(op);
                27
            }
            // add vx byte
            (7, x, ..) => {
                self.v[x as usize] = self.v[x as usize].wrapping_add(nn(op));
                45
            }
            // set vx vy
            (8, x, y, 0) => {
                self.v[x as usize] = self.v[y as usize];
                200
            }
            // or vx vy
            (8, x, y, 1) => {
                self.v[x as usize] |= self.v[y as usize];
                self.v[0xF] = 0;
                200
            }
            // and vx vy
            (8, x, y, 2) => {
                self.v[x as usize] &= self.v[y as usize];
                self.v[0xF] = 0;
                200
            }
            // xor vx vy
            (8, x, y, 3) => {
                self.v[x as usize] ^= self.v[y as usize];
                self.v[0xF] = 0;
                200
            }
            // add vx vy
            (8, x, y, 4) => {
                let (res, overflow) = self.v[x as usize].overflowing_add(self.v[y as usize]);
                self.v[x as usize] = res;
                self.v[0xF] = overflow as u8;
                200
            }
            // sub vx vy
            (8, x, y, 5) => {
                let (res, borrow) = self.v[x as usize].overflowing_sub(self.v[y as usize]);
                self.v[x as usize] = res;
                self.v[0xF] = !borrow as u8;
                200
            }
            // shr vx
            (8, x, y, 6) => {
                self.v[x as usize] = self.v[y as usize];
                let lsb = self.v[x as usize] & 1;
                self.v[x as usize] >>= 1;
                self.v[0xF] = lsb;
                200
            }
            // sub vx vy
            (8, x, y, 7) => {
                let (res, borrow) = self.v[y as usize].overflowing_sub(self.v[x as usize]);
                self.v[x as usize] = res;
                self.v[0xF] = !borrow as u8;
                200
            }
            // shl vx
            (8, x, y, 0xE) => {
                self.v[x as usize] = self.v[y as usize];
                let msb = (self.v[x as usize] >> 7) & 1;
                self.v[x as usize] <<= 1;
                self.v[0xF] = msb;
                200
            }
            // sne vx, vy
            (9, x, y, 0) => {
                if self.v[x as usize] != self.v[y as usize] {
                    self.pc += 2
                }
                73
            }
            // ld i nnn
            (0xA, ..) => {
                self.ir = op & 0x0FFF;
                55
            }
            // jp v0 nnn
            (0xB, ..) => {
                self.pc = self.v[0] as u16 + nnn(op);
                105
            }
            // rnd vx nn
            (0xC, x, ..) => {
                self.v[x as usize] = rand::random::<u8>() & nn(op);
                164
            }
            // drw vx vy n
            (0xD, x, y, n) => {
                let x_coord = (self.v[x as usize] % SCREEN_WIDTH as u8) as u16;
                let y_coord = (self.v[y as usize] % SCREEN_HEIGHT as u8) as u16;

                let mut flipped = false;
                for y_line in 0..n {
                    let addr = self.ir + y_line;
                    let pixels = self.mem[addr as usize];
                    for x_line in 0..8 {
                        if (pixels & (0b1000_0000 >> x_line)) != 0 {
                            let x = (x_coord + x_line) as usize;
                            let y = (y_coord + y_line) as usize;
                            let idx = x + SCREEN_WIDTH * y;
                            if let Some(pixel) = self.screen.get_mut(idx) {
                                flipped |= *pixel;
                                *pixel ^= true;
                            }
                        }
                    }
                }
                self.v[0xF] = flipped as u8;

                22734
            }
            // skp vx
            (0xE, x, 9, 0xE) => {
                if self.keypad[self.v[x as usize] as usize] {
                    self.pc += 2;
                }
                73
            }
            // sknp vx
            (0xE, x, 0xA, 1) => {
                if !self.keypad[self.v[x as usize] as usize] {
                    self.pc += 2;
                }
                73
            }
            // ld vx dt
            (0xF, x, 0, 7) => {
                self.v[x as usize] = self.dt;
                45
            }
            // ld vx k
            (0xF, x, 0, 0xA) => {
                let mut pressed = false;
                #[allow(clippy::needless_range_loop)]
                for i in 0..self.keypad.len() {
                    if self.keypad[i] {
                        pressed = true;
                        self.v[x as usize] = i as u8;
                        // wait for release

                        break;
                    }
                }
                if !pressed {
                    self.pc -= 2;
                }
                100
            }
            // ld dt vx
            (0xF, x, 1, 5) => {
                self.dt = self.v[x as usize];
                45
            }
            // ld st vx
            (0xF, x, 1, 8) => {
                self.st = self.v[x as usize];
                45
            }
            // add ir vx
            (0xF, x, 1, 0xE) => {
                self.ir = self.ir.wrapping_add(self.v[x as usize] as u16);
                86
            }
            // ld f vx
            (0xF, x, 2, 9) => {
                self.ir = self.v[x as usize] as u16 * 5;
                91
            }
            // ld b cx
            (0xF, x, 3, 3) => {
                let vx = self.v[x as usize];
                self.mem[self.ir as usize] = (vx / 100) % 10;
                self.mem[self.ir as usize + 1] = (vx / 10) % 10;
                self.mem[self.ir as usize + 2] = vx % 10;
                927
            }
            // ld ir vx
            (0xF, x, 5, 5) => {
                for offset in 0..=(x as usize) {
                    self.mem[self.ir as usize + offset] = self.v[offset];
                }
                self.ir += 1;
                605
            }
            // ld vx ir
            (0xF, x, 6, 5) => {
                for offset in 0..=(x as usize) {
                    self.v[offset] = self.mem[self.ir as usize + offset];
                }
                self.ir += 1;
                605
            }
            _ => todo!("unimplemented opcode: {op:04x}"),
        }
    }
}

const fn nn(op: u16) -> u8 {
    (op & 0x00FF) as u8
}
const fn nnn(op: u16) -> u16 {
    op & 0x0FFF
}
