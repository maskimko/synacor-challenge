use colored::Colorize;
use log::{debug, error, info, trace};
use std::error::Error;
use std::fmt::{self, Formatter};
use std::iter;
use std::collections::VecDeque;
use std::os::raw;

pub mod config;

//const MAX: u16 = 32768; // The same as 1 << 15
const MAX: u16 = 1 << 15;
struct VM {
    halt: bool,
    memory: [u8; 1 << 16], // as there is 15 bit address space, but each address points to the 2
    // bytes, so we actually need 15 bit * 2 address space for the memory array.
    registers: [u16; 8],
    stack: VecDeque<u16>,
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

    fn next(&self) -> Self {
        self.add(1)
    }
    fn add(&self, n: u16) -> Self {
        Address::new(self.0 + n)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let ptr: Ptr = self.into();
        write!(f, "addr[{} ({:#x})]", self.0, ptr)
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
impl Data {
    fn is_register(&self) -> bool {
        if let Data::Register(_) = self {
            true
        } else {
            false
        }
    }
    fn is_literal(&self) -> bool {
        if let Data::LiteralValue(_) = self {
            true
        } else {
            false
        }
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Data::Register(r) => write!(f, "register[{}]", r),
            Data::LiteralValue(v) => write!(f, "value[{}]", v),
        }
    }
}
/// This function composes u16 number from little endian byte pair of low byte and high byte
fn compose_value(byte_pair: (u8, u8)) -> u16 {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    let lb: u16 = byte_pair.0 as u16;
    let hb: u16 = (byte_pair.1 as u16) << 8;
    // Let's try not perform mod operation on this level
    // let value = (hb + lb) % MAX;
    // This was a bug preventing from getting register number!
    // The real mod '%' operation will happen at 'get_data_from_raw_value' function
    let value = hb + lb;
    trace!(
        "  compose value {} ({:#x}) from bytes {:?} ({:#x}, {:#x})",
        value, value, byte_pair, byte_pair.0, byte_pair.1
    );
    // If the value is greater than 32768 + 8 (MAX + number of registers), it will cause panic
    // anyway, so it makes sense to log it early
    if value > MAX + 8 {
        trace!(
            "  {} detected on composed value {} ({:#x})",
            "OVERFLOW".yellow(),
            value,
            value
        );
    }
    assert!(
        validate_value(value),
        "value bigger than 32768 + 8 is invalid"
    );
    value
}
/// This function decomposes u16 number to the little endian byte pair of low byte and high byte
fn decompose_value(value: u16) -> (u8, u8) {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    assert!(
        validate_value(value),
        "value bigger than 32768 + 8 is invalid"
    );
    let lb: u8 = (value % 8) as u8;
    let hb: u8 = (value >> 8) as u8;
    let byte_pair = (lb, hb);
    trace!(
        "  decompose bytes {:?} ({:#x}, {:#x}) from value {} ({:#x}) ",
        byte_pair, byte_pair.0, byte_pair.1, value, value
    );
    return byte_pair;
}

fn validate_value(val: u16) -> bool {
    val < MAX + 8
}
/// This method takes a provided value validates it and packs it to Data
fn pack_raw_value(v: u16) -> Data {
    let data = match v {
        val if v < MAX => {
            trace!("  packing literal value '{}'", v);
            Data::LiteralValue(val)
        }
        r if r % MAX < 8 => {
            let reg = (r % MAX) as usize;
            trace!("  packing register number value '{}' as reg: ({})", v, reg);
            Data::Register(reg)
        }
        // Probably we can just return an error here
        _ => panic!("values bigger than 32776 are invalid"),
    };
    data
}
/// This function just converts Data to raw memory address
fn unpack_data_to_raw_address(d: Data) -> u16 {
    let raw = match d {
        Data::LiteralValue(v) => v,
        Data::Register(r) => MAX + r as u16,
    };

    assert!(
        validate_value(raw),
        "value bigger than 32768 + 8 is invalid"
    );
    raw
}

impl VM {
    fn new() -> Self {
        VM {
            halt: false,
            memory: [0; 1 << 16],
            registers: [0; 8],
            stack: VecDeque::new(),
            current_address: Address::default(),
        }
    }
    fn show_state(&self) {
        eprintln!("***         Virtual Machine State         ***");
        eprintln!("{}", iter::repeat("=").take(44).collect::<String>());
        eprintln!("{:<9}: {}", "halt", self.halt);
        eprintln!("{:<9}: {}", "rom size", self.memory.len());
        self.show_registers(1);
        self.show_stack(1);
        eprintln!("{:<9}: {}", "position", self.current_address);
        eprintln!("=============================================");
    }
    fn show_registers(&self, indent: usize) {
        let indentation = iter::repeat("  ").take(indent).collect::<String>();
        eprintln!("{:<9}:", "registers");
        eprintln!(
            "{}{}",
            indentation,
            iter::repeat("-").take(44 - indent).collect::<String>()
        );
        self.registers
            .iter()
            .enumerate()
            .for_each(|(n, r)| eprintln!("{}{}{}: {:<10}", indentation, "reg ", n, r));
        eprintln!(
            "{}{}",
            indentation,
            iter::repeat("-").take(44 - indent).collect::<String>()
        );
    }
    fn show_stack(&self, indent: usize) {
        let indentation = iter::repeat("  ").take(indent).collect::<String>();
        eprintln!("{:<9}  (size: {:3}):", "stack", self.stack.len());
        eprintln!(
            "{}{}",
            indentation,
            iter::repeat("+").take(44 - indent).collect::<String>()
        );
        self.stack
            .iter()
            .enumerate()
            .rev()
            .for_each(|(n, r)| eprintln!("{}[{}: {:<10}]", indentation, n, r));
        eprintln!(
            "{}{}",
            indentation,
            iter::repeat("+").take(44 - indent).collect::<String>()
        );
    }
    fn new_from_rom(rom: Vec<u8>) -> Self {
        let mut vm = Self::new();
        vm.load_rom(rom);
        vm
    }
    fn load_rom(&mut self, rom: Vec<u8>) {
        debug!("loading program of {} bytes into memory", rom.len());
        for (n, v) in rom.into_iter().enumerate() {
            self.memory[n] = v;
        }
        trace!("loading OK!");
    }
    /// This method gets 2 adjasent bytes from the RAM and composes a number u16 from it
    fn get_value_from_addr(&self, addr: &Address) -> u16 {
        trace!(" getting value from address {}", addr);
        let ptr = addr.into();
        let lb = self.get_byte_value_from_ptr(ptr);
        let hb = self.get_byte_value_from_ptr(ptr + 1);
        compose_value((lb, hb))
    }
    /// This method gets raw memory value by pointer
    fn get_byte_value_from_ptr(&self, ptr: Ptr) -> u8 {
        let b = self.memory[ptr as usize];
        trace!(
            "  fetched {} [{:#x}] from memory pointer {} [{:#x}] ",
            b, b, ptr, ptr
        );
        b
    }

    fn get_data(&self, v: u16) -> u16 {
        self.unpack_data(pack_raw_value(v))
    }

    fn get_data_from_addr(&self, addr: Address) -> u16 {
        let v = self.get_value_from_addr(&addr);
        self.get_data(v)
    }

    fn get_from_register(&self, register: usize) -> u16 {
        if register >= 8 {
            panic!(
                "invalid register value {} There is 8 resisters only.",
                register
            );
        }
        let v = self.registers[register];
        trace!(" getting value {} from register {}", v, register);
        v
    }
    /// This method extracts data from both variants of Data enum
    fn unpack_data(&self, data: Data) -> u16 {
        let val = match data {
            Data::LiteralValue(lv) => lv,
            Data::Register(r) => self.get_from_register(r),
        };
        trace!(" unpacked value {} from {}", val, data);
        val
    }

    fn set_position(&mut self, pos: Address) {
        trace!("{}", format!("set position to {}", pos).yellow().italic());
        self.current_address = pos;
    }

    fn step(&mut self) {
        let next_address = self.current_address.next();
        trace!(
            "{} stepping to the next address {}",
            &self.current_address, next_address
        );
        self.set_position(next_address);
    }
    fn step_n(&mut self, n: u16) {
        let new_address = self.current_address.add(n);
        trace!(
            "{} stepping {} addresses forward to {}",
            &self.current_address, n, &new_address
        );
        self.set_position(new_address);
    }
    // Here  ops functions go
    fn noop(&mut self) {
        debug!("{} {}:", &self.current_address, "noop".magenta());
        self.step();
    }
    fn halt(&mut self) {
        debug!("{} {}:", &self.current_address, "halt".magenta());
        self.halt = true;
        info!("VM has been halt");
    }
    fn out(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "out".magenta(), &a);
        let character = self.get_data_from_addr(a) as u8 as char;
        trace!(
            "printing character '{}' ({:#x})",
            character.to_string().red(),
            character as u8
        );
        print!("{}", character);

        self.step_n(2);
    }

    fn jmp(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "jmp".magenta(), &a);
        let pos = Address::new(self.get_data_from_addr(a));
        self.set_position(pos);
    }
    fn jmp_true(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "jt".magenta(),
            &a,
            &b
        );
        if self.get_data_from_addr(a) != 0 {
            let pos = Address::new(self.get_data_from_addr(b));
            self.set_position(pos);
        } else {
            self.step_n(3);
        }
    }
    fn jmp_false(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "jf".magenta(),
            &a,
            &b
        );
        if self.get_data_from_addr(a) == 0 {
            let pos = Address::new(self.get_data_from_addr(b));
            self.set_position(pos);
        } else {
            self.step_n(3);
        }
    }
    fn set_register(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "set".magenta(),
            &a,
            &b
        );
        let reg_value = self.get_value_from_addr(&a);
        let reg = pack_raw_value(reg_value);
        assert!(
            reg.is_register(),
            "obtained value cannot be used as register"
        );
        let raw_value = self.get_value_from_addr(&b);
        let val = pack_raw_value(raw_value);
        assert!(
            val.is_literal(),
            "obtained value cannot be used as a literal value"
        );
        self.set_value_to_register(reg, val);
        self.step_n(3);
    }
    fn set_value_to_register(&mut self, reg: Data, val: Data) {
        trace!("setting value: {} to register: {}", val, reg);
        assert!(
            reg.is_register(),
            "obtained value cannot be used as register"
        );
        // Ensure that data is resolved, to prevent setting register to register
        let literal = self.unpack_data(val);
        // assert!(
        //     val.is_literal(),
        //     "obtained value cannot be used as a literal value"
        // );
        if let Data::Register(r) = reg {
            self.store_raw_value_to_register(r, literal);
        } else {
            panic!("failed to unpack register and its value")
        }
    }

    fn store_raw_value_to_register(&mut self, register_number: usize, value: u16) {
        assert!(register_number < 8);
        assert!(value < MAX + 8); // Here I tollerate storing register pointer values. Probably it
        // is a mistake
        trace!("storing value {} to register {}", value, register_number);
        self.registers[register_number] = value;
    }

    fn add(&mut self, a: Address, b: Address, c: Address) {
        debug!(
            "{} {}: {} {} {}",
            &self.current_address,
            "add".magenta(),
            &a,
            &b,
            &c
        );
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let value1 = pack_raw_value(self.get_value_from_addr(&b));
        let value2 = pack_raw_value(self.get_value_from_addr(&c));
        self.add_values(reg, value1, value2);
        self.step_n(4);
    }

    fn add_values(&mut self, reg: Data, v1: Data, v2: Data) {
        trace!(
            " storing result of add operation on {} and {} to {}",
            v1, v2, reg
        );

        assert!(
            reg.is_register(),
            "first argument value cannot be used as register"
        );
        let val1 = self.unpack_data(v1);
        let val2 = self.unpack_data(v2);
        if let Data::Register(r) = reg {
            self.store_raw_value_to_register(r, (val1 + val2) % MAX);
        } else {
            panic!("cannot unpack values and register for add operation");
        }
    }

    fn eq(&mut self, a: Address, b: Address, c: Address) {
        debug!(
            "{} {}: {} {} {}",
            &self.current_address,
            "eq".magenta(),
            &a,
            &b,
            &c
        );
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let value1 = pack_raw_value(self.get_value_from_addr(&b));
        let value2 = pack_raw_value(self.get_value_from_addr(&c));
        if self.store_equality(reg, value1, value2) {
            trace!("successfully stored positive result of comparison");
        } else {
            trace!("successfully stored negative result of comparison");
        }
        self.step_n(4);
    }

    fn store_equality(&mut self, reg: Data, v1: Data, v2: Data) -> bool {
        trace!(
            " storing result of eq operation of {} and {} to {}",
            v1, v2, reg
        );
        assert!(
            reg.is_register(),
            "first argument value cannot be used as register"
        );
        let val1 = self.unpack_data(v1);
        let val2 = self.unpack_data(v2);
        trace!(" comparing values {} and {}", val1, val2);
        if let Data::Register(r) = reg {
            if val1 == val2 {
                self.store_raw_value_to_register(r, 1);
                true
            } else {
                self.store_raw_value_to_register(r, 0);
                false
            }
        } else {
            panic!("cannot unpack values and register for add operation");
        }
    }

    fn push(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "push".magenta(), &a);
        let val = self.get_value_from_addr(&a);
        // I'm not sure wherer I should resolve value in the register or not...
        // let val = self.get_data_from_addr(a);
        self.stack.push_back(val);
        trace!("pushed value {} to stack", val);
        self.step_n(2);
    }

    fn pop(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "pop".magenta(), &a);
        let val = self.stack.pop_back().expect("stack is empty");
        trace!("popped value {} from stack", val);
        self.set_memory_by_address(a, val);
        self.step_n(2);
    }

    fn set_memory_by_address(&mut self, a: Address, val: u16) {
        trace!(" setting memory by address {} to {}", &a, val);
                let ptr: Ptr = (&a).into();
                self.set_memory(ptr, val);
    }
    fn set_memory(&mut self, ptr: Ptr, val: u16) {
        trace!(
            "  setting value: {} to memory raw ptr: {}({:#x})",
            val, ptr, ptr
        );
        assert!(
            validate_value(val),
            "value bigger than 32768 + 8 is invalid"
        );
        assert_eq!(
            (ptr as u16 % 2),
            0,
            "first pointer must point to an even address"
        );
        let (lb, hb) = decompose_value(val);
        self.memory[ptr as usize] = lb;
        self.memory[ptr as usize + 1] = hb;
    }

    fn main_loop(&mut self) -> Result<u64, Box<dyn Error>> {
        trace!("starting the main loop");
        let mut cycles: u64 = 0;

        loop {
            if self.halt {
                self.show_state();
                break;
            }
            // Debugging 
            self.show_state();
            cycles += 1;
            let current_val = self.get_value_from_addr(&self.current_address);
            let v = self.get_data(current_val);
            match v {
                0 => {
                    /*
                    halt: 0
                      stop execution and terminate the program
                    */
                    self.halt();
                }
                1 => {
                    /*
                    set: 1 a b
                      set register <a> to the value of <b>
                    */
                    self.set_register(self.current_address.add(1), self.current_address.add(2));
                }
                2 => {
                    /*
                    push: 2 a
                      push <a> onto the stack
                    */
                    self.push(self.current_address.add(1));
                }
                3 => {
                    /*
                    pop: 3 a
                      remove the top element from the stack and write it into <a>; empty stack = error
                    */
                    self.pop(self.current_address.add(1));
                    //    self.show_state();
                }
                4 => {
                    /*
                    eq: 4 a b c
                      set <a> to 1 if <b> is equal to <c>; set it to 0 otherwise
                    */
                    self.eq(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                5 => {
                    /*
                    gt: 5 a b c
                      set <a> to 1 if <b> is greater than <c>; set it to 0 otherwise
                    */
                    unimplemented!();
                }
                6 => {
                    /*
                    jmp: 6 a
                      jump to <a>
                    */
                    self.jmp(self.current_address.add(1));
                }
                7 => {
                    /*
                    jt: 7 a b
                      if <a> is nonzero, jump to <b>
                    */
                    self.jmp_true(self.current_address.add(1), self.current_address.add(2));
                }
                8 => {
                    /*
                    jf: 8 a b
                      if <a> is zero, jump to <b>
                    */
                    self.jmp_false(self.current_address.add(1), self.current_address.add(2));
                }
                9 => {
                    /*
                                        add: 9 a b c
                      assign into <a> the sum of <b> and <c> (modulo 32768)
                    */
                    self.add(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                10 => {
                    /*
                                        mult: 10 a b c
                      store into <a> the product of <b> and <c> (modulo 32768)
                    */
                    unimplemented!();
                }
                11 => {
                    /*
                                        mod: 11 a b c
                      store into <a> the remainder of <b> divided by <c>
                    */
                    unimplemented!();
                }
                12 => {
                    /*
                                        and: 12 a b c
                      stores into <a> the bitwise and of <b> and <c>
                    */
                    unimplemented!();
                }
                13 => {
                    /*
                                        or: 13 a b c
                      stores into <a> the bitwise or of <b> and <c>
                    */
                    unimplemented!();
                }
                14 => {
                    /*
                                        not: 14 a b
                      stores 15-bit bitwise inverse of <b> in <a>
                    */
                    unimplemented!();
                }
                15 => {
                    /*
                                        rmem: 15 a b
                      read memory at address <b> and write it to <a>
                    */
                    unimplemented!();
                }
                16 => {
                    /*
                                        wmem: 16 a b
                      write the value from <b> into memory at address <a>
                    */
                    unimplemented!();
                }
                17 => {
                    /*
                        call: 17 a
                      write the address of the next instruction to the stack and jump to <a>
                    */
                    unimplemented!();
                }
                18 => {
                    /*
                        ret: 18
                      remove the top element from the stack and jump to it; empty stack = halt
                    */
                    unimplemented!();
                }
                19 => {
                    /*
                        out: 19 a
                      write the character represented by ascii code <a> to the terminal
                    */
                    self.out(self.current_address.add(1));
                }
                20 => {
                    /*
                        in: 20 a
                      read a character from the terminal and write its ascii code to <a>; it can be assumed that once input starts, it will continue until a newline is encountered; this means that you can safely read whole lines from the keyboard and trust that they will be fully read
                    */
                    unimplemented!();
                }
                21 => {
                    /*
                        noop: 21
                      no operation

                                unimplemented!("main loop is not implemented yet");
                    */
                    // TODO: Probably it worth to add fuctions for each operation...
                    self.noop();
                }
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
    debug!("{}", format!("received configuration {}", &config));
    if !config.is_valid() {
        return Err("configuration is invalid".into());
    }
    trace!("configuration has been successfully validated");
    let (rom, replay) = config.rom_n_replay();
    let mut vm = VM::new_from_rom(rom);
    let cycles = vm.main_loop()?;
    debug!("VM exited after completing {} cycles", cycles);
    Ok(())
}
