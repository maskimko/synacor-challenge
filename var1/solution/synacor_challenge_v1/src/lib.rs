use log::{debug, error, trace};
use std::error::Error;
use std::fmt::{self, Formatter};

pub mod config;

//const MAX: u16 = 32768; // The same as 1 << 15
const MAX: u16 = 1 << 15;
struct VM {
    halt: bool,
    memory: [u8; 1 << 15],
    registers: [u16; 8],
    stack: Vec<u16>,
    // - all numbers are unsigned integers 0..32767 (15-bit)
    // - all math is modulo 32768; 32758 + 15 => 5
    current_address: Address, // internal execution pointer
}

/*
== binary format ==
- each number is stored as a 16-bit little-endian pair (low byte, high byte)
- numbers 0..32767 mean a literal value
- numbers 32768..32775 instead mean registers 0..7
- numbers 32776..65535 are invalid
- programs are loaded into memory starting at address 0
- address 0 is the first 16-bit value, address 1 is the second 16-bit value, etc
*/

// Points to the u8 data value in the memory array
type Ptr = u16;

impl From<&Address> for Ptr {
    fn from(a: &Address) -> Self {
        (a.0 * 2) as Ptr
    }
}

struct Address(u16);

impl Default for Address {
    fn default() -> Self {
        Address(0)
    }
}

impl Address {
    fn new(value: u16) -> Self {
        if value < MAX {
            return Address(value);
        }
        panic!("invalid address value (value must be less than {})", MAX);
    }

    fn from_ptr(value: u16) -> Self {
        (value as Ptr).into()
    }

    fn next(&self) -> Self {
        self.add(1)
    }
    fn add(&self, n: u16) -> Self {
        Address::new(self.0 + n)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "addr[{}]", self.0)
    }
}

impl From<Ptr> for Address {
    // - address 0 is the first 16-bit value, address 1 is the second 16-bit value, etc
    // In other words address points into 2 consequtive u8 values in the memory
    fn from(p: Ptr) -> Self {
        if p % 2 == 1 {
            error!(
                "provided pointer {} must be even! the value will be floored to the lesser one",
                p
            );
            // For a moment just to spot the anomaly
            panic!(
                "provided pointer {} must be even! the value will be floored to the lesser one",
                p
            );
        }
        Address::new(p / 2)
    }
}

enum Data {
    LiteralValue(u16),
    Register(usize),
}
fn compose_value(byte_pair: (u8, u8)) -> u16 {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    let lb: u16 = byte_pair.0 as u16;
    let hb: u16 = (byte_pair.1 as u16) << 8;
    let value = (hb + lb) % MAX;
    trace!("compose value {} from bytes {:?}", value, byte_pair);
    value
}
fn decompose_value(value: u16) -> (u8, u8) {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    let lb: u8 = (value % 8) as u8;
    let hb: u8 = (value >> 8) as u8;
    let byte_pair = (lb, hb);
    trace!("decompose bytes {:?} from value {} ", byte_pair, value);
    return byte_pair;
}

impl VM {
    fn new() -> Self {
        VM { halt: false, memory: [0; 1<<15], registers: [0;8], stack: vec![], current_address: Address::default()}
    }
    fn new_from_rom(rom: Vec<u8>) -> Self {
        let mut vm  = Self::new();
        vm.load_rom(rom);
        vm
    }
    fn load_rom(&mut self, rom: Vec<u8>) {
        debug!("loading program of {} bytes into memory", rom.len());
        for (n, v) in rom.into_iter().enumerate(){
            self.memory[n] = v;
        }
        trace!("loading OK!");
    }
    fn get_value_from_addr(&self, addr: &Address) -> u16 {
        trace!("getting value from address {}", addr);
        let ptr = addr.into();
        let lb = self.get_byte_value_from_ptr(ptr);
        let hb = self.get_byte_value_from_ptr(ptr + 1);
        compose_value((lb, hb))
    }
    fn get_byte_value_from_ptr(&self, ptr: Ptr) -> u8 {
        let b = self.memory[ptr as usize];
        trace!("fetched {} form memory pointer {} ", b, ptr);
        b
    }
    fn get_data_from_raw_value(&self, v: u16) -> Data {
        let data = match v {
            val if v < MAX => {
                trace!("packing literal value {}", v);
                Data::LiteralValue(val)
            }
            r if r % MAX < 8 => {
                trace!("packing register number {}", v);
                Data::Register((r % MAX) as usize)
            }
            _ => panic!("values bigger than 32776 are invalid"),
        };
        data
    }

    fn get_data(&self, v: u16) -> u16 {
        match self.get_data_from_raw_value(v) {
            Data::LiteralValue(lv) => lv,
            Data::Register(r) => self.get_from_register(r),
        }
    }

    fn get_from_register(&self, register: usize) -> u16 {
        if register >= 8 {
            panic!(
                "invalid register value {} There is 8 resisters only.",
                register
            );
        }
        let v = self.registers[register];
        trace!("getting value {} from register {}", v, register);
        v
    }

    fn step(&mut self) {
        self.current_address = self.current_address.next();
    }
    fn step_n(&mut self, n: u16) {
        self.current_address = self.current_address.add(n);
    }
    fn main_loop(&mut self) -> Result<u64, Box<dyn Error>> {
        trace!("starting the main loop");
        let mut cycles: u64 = 0;

        loop {
            if self.halt {
                break;
            }
            cycles += 1;
            let current_val = self.get_value_from_addr(&self.current_address);
            let v = self.get_data(current_val);
            match v {
 0 => {
/*
halt: 0
  stop execution and terminate the program
*/
                    unimplemented!();
},
 1 => {
/*
set: 1 a b
  set register <a> to the value of <b>
*/
                    unimplemented!();
},
 2 => {
/*
push: 2 a
  push <a> onto the stack
*/
                    unimplemented!();
},
 3 => {
/*
pop: 3 a
  remove the top element from the stack and write it into <a>; empty stack = error
*/
                    unimplemented!();
},
 4 => {
/*
eq: 4 a b c
  set <a> to 1 if <b> is equal to <c>; set it to 0 otherwise
*/
                    unimplemented!();
},
 5 => {
/*
gt: 5 a b c
  set <a> to 1 if <b> is greater than <c>; set it to 0 otherwise
*/
                    unimplemented!();
},
 6 => {
/*
jmp: 6 a
  jump to <a>
*/
                    unimplemented!();
},
 7 => {
/*
jt: 7 a b
  if <a> is nonzero, jump to <b>
*/
                    unimplemented!();
},
 8 => {
/*
jf: 8 a b
  if <a> is zero, jump to <b>
*/
                    unimplemented!();
                },
 9 => {
/*
                    add: 9 a b c
  assign into <a> the sum of <b> and <c> (modulo 32768)
*/
                    unimplemented!();
                },
 10 => {
/*
                    mult: 10 a b c
  store into <a> the product of <b> and <c> (modulo 32768)
*/
                    unimplemented!();
                },
 11 => {
/*
                    mod: 11 a b c
  store into <a> the remainder of <b> divided by <c>
*/
                    unimplemented!();
                },
 12 => {
/*
                    and: 12 a b c
  stores into <a> the bitwise and of <b> and <c>
*/
                    unimplemented!();
                },
 13 => {
/*
                    or: 13 a b c
  stores into <a> the bitwise or of <b> and <c>
*/
                    unimplemented!();
                },
 14 => {
/*
                    not: 14 a b
  stores 15-bit bitwise inverse of <b> in <a>
*/
                    unimplemented!();
                },
 15 => {
/*
                    rmem: 15 a b
  read memory at address <b> and write it to <a>
*/
                    unimplemented!();
                },
 16 => {
/*
                    wmem: 16 a b
  write the value from <b> into memory at address <a>
*/
},
 17 => {
/*
    call: 17 a
  write the address of the next instruction to the stack and jump to <a>
*/
                    unimplemented!();
},
 18 => {
/*
    ret: 18
  remove the top element from the stack and jump to it; empty stack = halt
*/
                    unimplemented!();
},
 19 => {
/*
    out: 19 a
  write the character represented by ascii code <a> to the terminal
*/
                    unimplemented!();
},
 20 => {
/*
    in: 20 a
  read a character from the terminal and write its ascii code to <a>; it can be assumed that once input starts, it will continue until a newline is encountered; this means that you can safely read whole lines from the keyboard and trust that they will be fully read
*/
                    unimplemented!();
},
 21 => {
/*
    noop: 21
  no operation

            unimplemented!("main loop is not implemented yet");
*/
                    unimplemented!();
                },
                instruction => panic!("got invalid instruction {}", instruction),
            }
/*
== hints ==
- Start with operations 0, 19, and 21.
- Here's a code for the challenge website: ZjuGobDBMEiN
- The program "9,32768,32769,4,19,32768" occupies six memory addresses and should:
  - Store into register 0 the sum of 4 and the value contained in register 1.
  - Output to the terminal the character with the ascii code contained in register 0.

== opcode listing ==
halt: 0
  stop execution and terminate the program
set: 1 a b
  set register <a> to the value of <b>
push: 2 a
  push <a> onto the stack
pop: 3 a
  remove the top element from the stack and write it into <a>; empty stack = error
eq: 4 a b c
  set <a> to 1 if <b> is equal to <c>; set it to 0 otherwise
gt: 5 a b c
  set <a> to 1 if <b> is greater than <c>; set it to 0 otherwise
jmp: 6 a
  jump to <a>
jt: 7 a b
  if <a> is nonzero, jump to <b>
jf: 8 a b
  if <a> is zero, jump to <b>
add: 9 a b c
  assign into <a> the sum of <b> and <c> (modulo 32768)
mult: 10 a b c
  store into <a> the product of <b> and <c> (modulo 32768)
mod: 11 a b c
  store into <a> the remainder of <b> divided by <c>
and: 12 a b c
  stores into <a> the bitwise and of <b> and <c>
or: 13 a b c
  stores into <a> the bitwise or of <b> and <c>
not: 14 a b
  stores 15-bit bitwise inverse of <b> in <a>
rmem: 15 a b
  read memory at address <b> and write it to <a>
wmem: 16 a b
  write the value from <b> into memory at address <a>
call: 17 a
  write the address of the next instruction to the stack and jump to <a>
ret: 18
  remove the top element from the stack and jump to it; empty stack = halt
out: 19 a
  write the character represented by ascii code <a> to the terminal
in: 20 a
  read a character from the terminal and write its ascii code to <a>; it can be assumed that once input starts, it will continue until a newline is encountered; this means that you can safely read whole lines from the keyboard and trust that they will be fully read
noop: 21
  no operation
*/
        }

        Ok(cycles)
}
}

pub fn run(config: config::Configuration) -> Result<(), Box<dyn Error>> {
    debug!("received configuration {:?}", config);
    if !config.is_valid() {
        return Err("configuration is invalid".into())
    }
    trace!("configuration has been successfully validated");
    let (rom, replay) = config.rom_n_replay();
    let mut vm  = VM::new_from_rom(rom);
    let cycles  = vm.main_loop()?;
    debug!("VM exited after completing {} cycles", cycles);
    Ok(())
}
