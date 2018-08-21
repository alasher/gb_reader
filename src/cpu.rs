#![allow(dead_code)]

use memory::Memory;
use lookup::Instruction;
use registers::RegisterCache;
use registers::Reg8;
use registers::Reg16;
use registers::RegOps;
use util;
use lookup;

enum AluOp {
    Add,
    AddCarry,
    Sub,
    SubCarry,
    And,
    Xor,
    Or,
    Comp
}

pub struct CPU {
    regs: RegisterCache,
    mem: Memory,
    ir_enabled: bool,
    quit: bool
}

impl CPU {
    pub fn new(mem: Memory) -> CPU {
        CPU {
            regs: RegisterCache::new(),
            mem: mem,
            ir_enabled: true,
            quit: false
        }
    }

    pub fn get_pc(&self) -> u16 {
        self.regs.get(Reg16::PC)
    }

    // Get the u16 value starting at $(addr), little endian.
    fn parse_u16(&self, addr: u16) -> u16 {
        util::join_u8((self.mem.get(addr), self.mem.get(addr+1)))
    }

    // Push addr from given register onto stack
    fn push(&mut self, src: Reg16) {
        self.regs.sub(Reg16::SP, 2);
        let sp_val = self.regs.get(Reg16::SP);
        let split_addr = util::split_u16(self.regs.get(src));
        self.mem.set(split_addr.0, sp_val);
        self.mem.set(split_addr.1, sp_val+1);
    }

    // Pop topmost u16 value from stack, store to given register
    fn pop(&mut self, dst: Reg16) {
        let stack_val = self.parse_u16(self.regs.get(Reg16::SP));
        self.regs.add(Reg16::SP, 2);
        self.regs.set(dst, stack_val);
    }

    // Push next_addr to stack, and jump to the jump_addr
    fn call(&mut self, jump_addr: u16) {
        self.push(Reg16::PC);
        self.regs.set(Reg16::PC, jump_addr);
    }

    // Pop the topmost address from the stack, and jump to it.
    fn ret(&mut self, enable_ir: bool) {
        self.pop(Reg16::PC);
        if enable_ir {
            self.ir_enabled = true;
        }
    }

    // Copy from given register into the memory address pointed to by given Reg16
    fn set_reg_ptr(&mut self, dst: Reg16, src: Reg8) {
        let val = self.regs.get(src);
        self.mem.set(val, self.regs.get(dst));
    }

    // Copy value from (HL) into given register.
    fn get_reg_ptr(&mut self, dst: Reg8, src: Reg16) {
        let val = self.mem.get(self.regs.get(src));
        self.regs.set(dst, val);
    }

    // Copy value between A and (HL), then add or subtract HL.
    fn ldd_special(&mut self, is_get: bool, is_add: bool) {
        if is_get {
            self.get_reg_ptr(Reg8::A, Reg16::HL); // LD A, (HL+/-)
        } else {
            self.set_reg_ptr(Reg16::HL, Reg8::A); // LD (HL+/-), A
        }

        if is_add {
            self.regs.add(Reg16::HL, 1);
        } else {
            self.regs.sub(Reg16::HL, 1);
        }
    }

    fn ld_fast_page(&mut self, is_get: bool) {
        let addr = 0xFF00 + self.regs.get(Reg8::C) as u16;
        if is_get {
            self.regs.set(Reg8::A, self.mem.get(addr));
        } else {
            self.mem.set(self.regs.get(Reg8::A), addr);
        }
    }

    // Increment/decrement for (HL) value. TODO: should this be done another way? Maybe implement
    // it as a Reg8::HL_PTR, or something special in ALU?
    fn hl_ptr_inc_dec(&mut self, is_add: bool) {
        let addr = self.regs.get(Reg16::HL);
        let val = self.mem.get(addr);
        let val = if is_add { val + 1 } else { val - 1};
        self.mem.set(val, addr);
    }

    // Jump relative to current PC, where offset is twos-complement 8-bit signed int.
    fn jump_relative(&mut self, offset: u8) {
        let addr = self.regs.get(Reg16::PC) as i32;
        let addr = addr + (offset as i8) as i32;
        if addr < 0 || addr > 0xFFFF {
            println!("Fatal error: jumped out-of-bounds!");
            self.quit = true;
            return;
        }

        self.regs.set(Reg16::PC, addr as u16);
    }

    // Perform given ALU instruction with the given argument
    fn alu(&mut self, op: AluOp, val: u8) {
        let a = self.regs.get(Reg8::A);
        let cy = 0; // TODO: Retrieve this from register cache
        let result = match op {
            AluOp::Add      => a + val,
            AluOp::AddCarry => a + val + cy,
            AluOp::Sub      => a - val,
            AluOp::SubCarry => a - val - cy,
            AluOp::And      => a & val,
            AluOp::Xor      => a ^ val,
            AluOp::Or       => a | val,
            AluOp::Comp     => !val
        };

        self.regs.set(Reg8::A, result);

        // TODO: Add flag mods here
    }

    // For HALT, just exit the program for now. TODO: Add accurate HALT emulation here.
    fn halt(&mut self) {
        println!("Encountered HALT instruction, exiting!");
        self.quit = true;
    }

    fn stop(&mut self) {
        println!("Encountered STOP instruction, exiting!");
        self.quit = true;
    }

    // Run the instruction at the current PC, return true if successful.
    pub fn process(&mut self) -> bool {
        if self.quit { return false; }
        let old_pc = self.regs.get(Reg16::PC);
        let opcode = self.mem.get(old_pc);
        let _operand8  = self.mem.get(old_pc+1);
        let _operand16 = self.parse_u16(old_pc+1);

        // Adjust opcode if it's a 0xCB prefixed instruction
        let opcode = if opcode == 0xCB {
            let newop = 0xCB as u16 | _operand8 as u16;
            let _operand8  = self.mem.get(old_pc+2);
            let _operand16 = self.parse_u16(old_pc+2);
            newop
        } else {
            opcode as u16
        };

        let inst = lookup::get_instruction(opcode);

        // Increment PC before we process the instruction. During execution the current PC will
        // represent the next instruction to process.
        let mut bytes = inst.bytes as u16;
        if inst.prefix_cb {
            bytes += 1; // TODO: Fix this in the lookup table
        }
        self.regs.set(Reg16::PC, old_pc + bytes);

        // Print info about this instruction. Leaving this on all the time until the software
        // matures a little. development
        self.print_instruction_info(&inst, old_pc);

        match opcode {
            // [0x00, 0x3F] - Load, INC/DEC, some jumps, and other various instructions.
            0x00 => (),
            0x01 => self.regs.set(Reg16::BC, _operand16),
            0x02 => self.set_reg_ptr(Reg16::BC, Reg8::A),
            0x03 => self.regs.add(Reg16::BC, 1),
            0x04 => self.regs.add(Reg8::B, 1),
            0x05 => self.regs.sub(Reg8::B, 1),
            0x06 => self.regs.set(Reg8::B, _operand8),
            0x0A => self.get_reg_ptr(Reg8::A, Reg16::BC),
            0x0B => self.regs.sub(Reg16::BC, 1),
            0x0C => self.regs.add(Reg8::C, 1),
            0x0D => self.regs.sub(Reg8::C, 1),
            0x0E => self.regs.set(Reg8::C, _operand8),
            0x10 => self.stop(),
            0x11 => self.regs.set(Reg16::DE, _operand16),
            0x12 => self.set_reg_ptr(Reg16::DE, Reg8::A),
            0x13 => self.regs.add(Reg16::DE, 1),
            0x14 => self.regs.add(Reg8::D, 1),
            0x15 => self.regs.sub(Reg8::D, 1),
            0x16 => self.regs.set(Reg8::D, _operand8),
            0x18 => self.jump_relative(_operand8),
            0x1A => self.get_reg_ptr(Reg8::A, Reg16::DE),
            0x1B => self.regs.sub(Reg16::BC, 1),
            0x1C => self.regs.add(Reg8::D, 1),
            0x1D => self.regs.sub(Reg8::D, 1),
            0x1E => self.regs.set(Reg8::E, _operand8),
            0x21 => self.regs.set(Reg16::HL, _operand16),
            0x22 => self.ldd_special(true, true),
            0x23 => self.regs.add(Reg16::HL, 1),
            0x24 => self.regs.add(Reg8::H, 1),
            0x25 => self.regs.sub(Reg8::H, 1),
            0x26 => self.regs.set(Reg8::H, _operand8),
            0x2A => self.ldd_special(false, true),
            0x2B => self.regs.sub(Reg16::HL, 1),
            0x2C => self.regs.add(Reg8::L, 1),
            0x2D => self.regs.sub(Reg8::L, 1),
            0x2E => self.regs.set(Reg8::L, _operand8),
            0x31 => self.regs.set(Reg16::SP, _operand16),
            0x32 => self.ldd_special(true, false),
            0x33 => self.regs.add(Reg16::HL, 1),
            0x34 => self.hl_ptr_inc_dec(true),
            0x35 => self.hl_ptr_inc_dec(false),
            0x36 => self.mem.set(_operand8, self.regs.get(Reg16::HL)),
            0x3A => self.ldd_special(false, false),
            0x3B => self.regs.sub(Reg16::SP, 1),
            0x3C => self.regs.add(Reg8::A, 1),
            0x3D => self.regs.sub(Reg8::A, 1),
            0x3E => self.regs.set(Reg8::A, _operand8),

            // [0x40, 0x7F] - Mostly copy instructions between registers and (HL).
            0x40 => self.regs.copy(Reg8::B, Reg8::B),
            0x41 => self.regs.copy(Reg8::B, Reg8::C),
            0x42 => self.regs.copy(Reg8::B, Reg8::D),
            0x43 => self.regs.copy(Reg8::B, Reg8::E),
            0x44 => self.regs.copy(Reg8::B, Reg8::H),
            0x45 => self.regs.copy(Reg8::B, Reg8::L),
            0x46 => self.get_reg_ptr(Reg8::B, Reg16::HL),
            0x47 => self.regs.copy(Reg8::B, Reg8::A),
            0x48 => self.regs.copy(Reg8::C, Reg8::B),
            0x49 => self.regs.copy(Reg8::C, Reg8::C),
            0x4a => self.regs.copy(Reg8::C, Reg8::D),
            0x4b => self.regs.copy(Reg8::C, Reg8::E),
            0x4c => self.regs.copy(Reg8::C, Reg8::H),
            0x4d => self.regs.copy(Reg8::C, Reg8::L),
            0x4e => self.get_reg_ptr(Reg8::C, Reg16::HL),
            0x4f => self.regs.copy(Reg8::C, Reg8::A),
            0x50 => self.regs.copy(Reg8::D, Reg8::B),
            0x51 => self.regs.copy(Reg8::D, Reg8::C),
            0x52 => self.regs.copy(Reg8::D, Reg8::D),
            0x53 => self.regs.copy(Reg8::D, Reg8::E),
            0x54 => self.regs.copy(Reg8::D, Reg8::H),
            0x55 => self.regs.copy(Reg8::D, Reg8::L),
            0x56 => self.get_reg_ptr(Reg8::D, Reg16::HL),
            0x57 => self.regs.copy(Reg8::D, Reg8::A),
            0x58 => self.regs.copy(Reg8::E, Reg8::B),
            0x59 => self.regs.copy(Reg8::E, Reg8::C),
            0x5a => self.regs.copy(Reg8::E, Reg8::D),
            0x5b => self.regs.copy(Reg8::E, Reg8::E),
            0x5c => self.regs.copy(Reg8::E, Reg8::H),
            0x5d => self.regs.copy(Reg8::E, Reg8::L),
            0x5e => self.get_reg_ptr(Reg8::E, Reg16::HL),
            0x5f => self.regs.copy(Reg8::E, Reg8::A),
            0x60 => self.regs.copy(Reg8::H, Reg8::B),
            0x61 => self.regs.copy(Reg8::H, Reg8::C),
            0x62 => self.regs.copy(Reg8::H, Reg8::D),
            0x63 => self.regs.copy(Reg8::H, Reg8::E),
            0x64 => self.regs.copy(Reg8::H, Reg8::H),
            0x65 => self.regs.copy(Reg8::H, Reg8::L),
            0x66 => self.get_reg_ptr(Reg8::H, Reg16::HL),
            0x67 => self.regs.copy(Reg8::H, Reg8::A),
            0x68 => self.regs.copy(Reg8::L, Reg8::B),
            0x69 => self.regs.copy(Reg8::L, Reg8::C),
            0x6a => self.regs.copy(Reg8::L, Reg8::D),
            0x6b => self.regs.copy(Reg8::L, Reg8::E),
            0x6c => self.regs.copy(Reg8::L, Reg8::H),
            0x6d => self.regs.copy(Reg8::L, Reg8::L),
            0x6e => self.get_reg_ptr(Reg8::L, Reg16::HL),
            0x6f => self.regs.copy(Reg8::L, Reg8::A),
            0x70 => self.set_reg_ptr(Reg16::HL, Reg8::B),
            0x71 => self.set_reg_ptr(Reg16::HL, Reg8::C),
            0x72 => self.set_reg_ptr(Reg16::HL, Reg8::D),
            0x73 => self.set_reg_ptr(Reg16::HL, Reg8::E),
            0x74 => self.set_reg_ptr(Reg16::HL, Reg8::H),
            0x75 => self.set_reg_ptr(Reg16::HL, Reg8::L),
            0x76 => self.halt(),
            0x77 => self.set_reg_ptr(Reg16::HL, Reg8::A),
            0x78 => self.regs.copy(Reg8::A, Reg8::B),
            0x79 => self.regs.copy(Reg8::A, Reg8::C),
            0x7a => self.regs.copy(Reg8::A, Reg8::D),
            0x7b => self.regs.copy(Reg8::A, Reg8::E),
            0x7c => self.regs.copy(Reg8::A, Reg8::H),
            0x7d => self.regs.copy(Reg8::A, Reg8::L),
            0x7e => self.get_reg_ptr(Reg8::A, Reg16::HL),
            0x7f => self.regs.copy(Reg8::A, Reg8::A),

            // [0x80, 0xBF] - Arithmetic operations

            // [0xC0, 0xFF] - Flow control, push/pop/call/ret, and other various instructions.
            0xC1 => self.pop(Reg16::BC),
            0xC3 => self.regs.set(Reg16::PC, _operand16),
            0xC5 => self.push(Reg16::BC),
            0xC7 => self.call(0x00),
            0xC9 => self.ret(false),
            0xCB => self.quit = true, // This shouldn't ever happen
            0xCD => self.call(_operand16),
            0xCF => self.call(0x08),
            0xD1 => self.pop(Reg16::DE),
            0xD5 => self.push(Reg16::DE),
            0xD7 => self.call(0x10),
            0xD9 => self.ret(true),
            0xDF => self.call(0x18),
            0xE0 => self.mem.set(self.regs.get(Reg8::A), 0xFF00 + (_operand8 as u16)),
            0xE1 => self.pop(Reg16::HL),
            0xE2 => self.ld_fast_page(true),
            0xE7 => self.call(0x20),
            0xEA => self.mem.set(self.regs.get(Reg8::A), _operand16),
            0xE5 => self.push(Reg16::HL),
            0xEF => self.call(0x28),
            0xF0 => self.regs.set(Reg8::A, self.mem.get(0xFF00 + (_operand8 as u16))),
            0xF1 => self.pop(Reg16::AF),
            0xF2 => self.ld_fast_page(false),
            0xF3 => self.ir_enabled = false,
            0xFA => self.regs.set(Reg8::A, self.mem.get(_operand16)),
            0xFB => self.ir_enabled = true,
            0xF5 => self.push(Reg16::AF),
            0xF7 => self.call(0x30),
            0xFF => self.call(0x38),
            _ => {
                println!("Fatal error: undefined instruction!");
                self.quit = true;
            }
        }

        !self.quit
    }

    fn print_instruction_info(&self, inst: &Instruction, old_pc: u16) {
        let mut pstr = format!("0x{:04x}: {} - {} cycles", old_pc, inst.name, inst.clocks);
        if inst.bytes > 1 {
            pstr += " - operands: ";
            for i in 1..inst.bytes {
                pstr += &format!("0x{:02x} ", self.mem.get(old_pc + i as u16));
            }
        }
        println!("{}", pstr);
    }
}
