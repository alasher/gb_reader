// PPU abstracts the details of the PPU from the CPU. It's different from the Window struct because
// the window abstracts platform-specific details related to operating the window.

use window::Window;
use memory::Memory;
use memory::MemClient;

use std::sync::Arc;

#[derive(Copy, Clone, PartialEq)]
enum PPUState {
    Off,
    HBlank,
    VBlank,
    OAMSearch,
    Draw
}

pub struct PPU {
    lcd: Window,      // The actual graphics window, not to be confused with a Game Boy window map/tile.
    state: PPUState,  // Current PPU state, non-off is STAT[0:1], OFF is controlled by LCDC bit 7.
    mem: Arc<Memory>, // Reference to our Memory object.
    width: u32,       // Width of the virtual window, fixed at 160.
    height: u32,      // Height of the virtual window, fixed at 144.
    ly: u32,          // The line we're currently on.
    lclk: u32,        // The machine cycle for this line, from [0, 113].
    lyc: u32,         // Value to compare to LY, can generate an interrupt.
    bgr_map_off: u16, // Offset to BG Map start address in VRAM, adjustble by LCDC bit 3.
    win_map_off: u16, // Offset to Window map start address in VRAM, adjustable by LCDC bit 6.
    bgr_dat_off: u16  // Offset to BG/Window data start address in VRAM, adjustable by LCDC bit 4.
}

impl PPU {
    pub fn new(mem: Arc<Memory>) -> Self {
        let (w, h) = (160, 144);
        let lcd = Window::new(w, h);
        PPU {
            lcd: lcd,
            state: PPUState::Off,
            mem: mem,
            width: w,
            height: h,
            ly: 0,
            lclk: 0,
            lyc: 0,
            bgr_map_off: 0,
            win_map_off: 0,
            bgr_dat_off: 0
        }
    }

    // Tick performs the appropriate PPU action for this machine cycle.
    // TODO: Adjust cycle accuracy of Draw state, timings can vary slightly.
    pub fn tick(&mut self) {
        match self.state {
            PPUState::Off => (),
            PPUState::HBlank => {
                if self.lclk == 113 {
                    if self.ly == 143 {
                        self.state = PPUState::VBlank;
                    } else {
                        self.state = PPUState::Draw;
                    }
                    self.ly += 1;
                    self.lclk = 0;
                } else {
                    self.lclk += 1;
                }
            },
            PPUState::VBlank => {
                if self.lclk == 113 {
                    if self.ly == 153 {
                        self.state = PPUState::OAMSearch;
                        self.ly = 0;
                    } else {
                        self.ly += 1;
                    }
                    self.lclk = 0;
                } else {
                    self.lclk += 1;
                }
            },
            PPUState::OAMSearch => {
                if self.lclk == 19 {
                    self.state = PPUState::Draw;
                }
                self.lclk += 1;
            },
            PPUState::Draw => {
                if self.lclk == 62 {
                    self.state = PPUState::HBlank;
                }
                self.lclk += 1;
            }
        }
    }

    pub fn start(&mut self) {
        self.state = PPUState::OAMSearch;
        self.lclk = 0;
        self.ly = 0;
        self.render();
    }

    fn render(&mut self) {
        // TODO: Right now pixel format is RGB8 (8 bits for each component)
        // This can probably be lowered once I know more about the CGB.
        let mut pixels = Vec::new();
        for w in 0..self.width {
            let pcolor = (w as f32 * 255f32 / self.width as f32) as u8;
            for h in 0..self.height {
                pixels.push(pcolor);
                pixels.push(pcolor);
                pixels.push(pcolor);
            }
        }

        if self.is_running() {
            self.lcd.get_events();
            if self.lcd.is_open() {
                // Set LY = 0
                self.lcd.draw(&pixels);
            } else {
                self.stop();
            }
        }
    }

    pub fn stop(&mut self) {
        self.state = PPUState::Off;
    }

    pub fn is_running(&self) -> bool {
        self.state != PPUState::Off
    }

    // VRAM data access, given absolute memory address
    // VRAM [0x8000, 0xa000) -> [0x0, 0x2000]
    // OAM RAM access [0xFE00, 0xFEA0) -> []
    fn get(&self, addr: u16) -> u8 {
        self.mem.get(addr, MemClient::PPU)
    }

    fn set(&mut self, val: u8, addr: u16) {
        Arc::get_mut(&mut self.mem).unwrap().set(val, addr, MemClient::PPU);
    }
}
