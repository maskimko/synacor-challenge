use log::{debug, error, trace};
use std::error::Error;
use std::fmt::{self, Formatter};

pub mod config;

//const MAX: u16 = 32768; // The same as 1 << 15
const MAX: u16 = 1<<15; 
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

impl From<Address> for Ptr {
    fn from(a : Address) -> Self {
        (a.0 * 2) as Ptr
    }
}

struct Address(u16);

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
        write!(f, "addr[{}]",self.0)
    }
}

impl From<Ptr> for Address {
    // - address 0 is the first 16-bit value, address 1 is the second 16-bit value, etc
    // In other words address points into 2 consequtive u8 values in the memory
    fn from(p: Ptr) -> Self {
        if p % 2 == 1 {
            error!( "provided pointer {} must be even! the value will be floored to the lesser one", p);
            // For a moment just to spot the anomaly
            panic!(                "provided pointer {} must be even! the value will be floored to the lesser one", p);
        }
        Address::new(p / 2)
    }
}


enum Data {
    LiteralValue(u16),
    Register(usize)
}
    fn compose_value(byte_pair: (u8, u8)) -> u16 {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    let lb :u16= byte_pair.0 as u16;
    let hb :u16= (byte_pair.1 as u16) << 8;
    let value = (hb + lb) % MAX;
    trace!("compose value {} from bytes {:?}", value, byte_pair)    ;
    value
    }
    fn decompose_value(value: u16) -> (u8, u8) {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    let lb: u8 = (value % 8) as u8;
    let hb : u8= (value >> 8) as u8;
    let byte_pair = (lb, hb);
    trace!("decompose bytes {:?} from value {} ",byte_pair , value)    ;
     return byte_pair;
    }

impl VM {

    fn get_value_from_addr(&self, addr: Address) -> u16 {
        trace!("getting value from address {}", addr);
        let ptr = addr.into();
        let lb = self.get_byte_value_from_ptr(ptr);
        let hb = self.get_byte_value_from_ptr(ptr+1);
        compose_value((lb, hb))
    }
    fn get_byte_value_from_ptr(&self, ptr: Ptr) -> u8 {
        let b  = self.memory[ptr as usize];
        trace!("fetched {} form memory pointer {} ", b, ptr);
        b
    }
    fn get_data_from_raw_value(&self, v: u16) -> Data {
        let data = match v {
            val if v < MAX => { 
                trace!("packing literal value {}", v);
                Data::LiteralValue(val)
            },
            r if r % MAX < 8 => { 
                trace!("packing register number {}", v);
                Data::Register((r%MAX) as usize)
            },
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
        if register >=8 {
            panic!("invalid register value {} There is 8 resisters only.", register);
        }
        let v = self.registers[register];
        trace!("getting value {} from register {}", v, register);
        v
    }

    fn step(&mut self) {
       self.current_address = self.current_address.next(); 
    }
    fn step_n(&mut self, n: u16) {
       self.current_address = self.current_address.add(n) ;
    }
    fn main_loop(&self) -> Result<u64, Box<dyn Error>> {

        let mut cycles : u64= 0;

        loop {
            cycles +=1 ;
            unimplemented!("main loop is not implemented yet");
        }

        Ok(cycles)
    }
}

pub fn run(config: config::Configuration) -> Result<(), Box<dyn Error>> {
    debug!("received configuration {:?}", config);
    debug!("starting the main loop");
    Ok(())
}
