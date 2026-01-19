use colored::Colorize;
use log::{Level, debug, error, info, trace};
use log::{log_enabled, warn};
use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use crate::aux::Commander;
use crate::maze_analyzer::{CommandType, MazeAnalyzer};

mod aux;
pub mod config;
mod maze_analyzer;
mod output_parser;

mod dot_graph;

//const MAX: u16 = 32768; // The same as 1 << 15
const MAX: u16 = 1 << 15;
const PRINT_WIDTH: usize = 44;
struct VM {
    halt: bool,
    memory: [u8; 1 << 16], // as there is 15 bit address space, but each address points to the 2
    // bytes, so we actually need 15 bit * 2 address space for the memory array.
    registers: [u16; 8],
    stack: VecDeque<u16>,
    // - all numbers are unsigned integers 0..32767 (15-bit)
    // - all math is modulo 32768; 32758 + 15 => 5
    current_address: Address, // internal execution pointer

    // Auxiliary stuff
    replay_commands: Option<Vec<String>>,
    replay_buffer: VecDeque<char>,
    commands_history: Vec<String>,
    record_output: Option<PathBuf>,
    current_command_buf: String, //used to store user input until the newline character
    output_writer: Option<BufWriter<File>>,
    maze_analyzer: MazeAnalyzer,
    spin_slash_command: bool,
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

#[derive(Default)]
struct Address(u16);

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        matches!(self, Data::Register(_))
    }
    #[allow(dead_code)]
    fn is_literal(&self) -> bool {
        matches!(self, Data::LiteralValue(_))
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Data::Register(r) => write!(f, "register[{}]", r),
            Data::LiteralValue(v) => write!(f, "value[{}]", v),
        }
    }
}
impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Data::Register(r) => write!(f, "register[{}]", r),
            Data::LiteralValue(v) => write!(f, "value[{}]", v),
        }
    }
}

fn print_slash_command_help() {
    eprintln!("{}", "*** Available slash '/' commands: ***".green());
    eprintln!("{:15} - {}", "/help".yellow(), "show this help");
    eprintln!("{:15} - {}", "/show_replay".yellow(), "show replay commands");
    eprintln!("{:15} - {}", "/show_state".yellow(), "show state of the VM");
    eprintln!(
        "{:15} - {}",
        "/dump_state".yellow(), "save VM state information to file"
    );
    eprintln!("{:15} - {}", "/dump_memory".yellow(), "save VM RAM to file");
    eprintln!("{:15} - {}", "/show_history".yellow(), "show commands history");
    eprintln!(
        "{:15} - {}",
        "/save_history".yellow(), "save commands history to file"
    );
    eprintln!("{:15} - {}", "/record_output".yellow(), "start output recording");
    eprintln!(
        "{:15} - {}",
        "/solve".yellow(), "steps limit] - start automatic path search (Default steps limit is 100)"
    );
    eprintln!(
        "{:15} - {}",
        "/show_path".yellow(), "show the shortest path back to start"
    );
    eprintln!(
        "{:15} - {}",
        "/dump_dot".yellow(), "dump visited noded graph in the .dot format to file"
    );
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
    // The real mod '%' operation will happen at 'pack_raw_value' function
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

fn char_is_printable(c: char) -> bool {
    c as u8 >= 32 && c as u8 <= 126
}

/// This function decomposes u16 number to the little endian byte pair of low byte and high byte
fn decompose_value(value: u16) -> (u8, u8) {
    // - all math is modulo 32768; 32758 + 15 => 5
    // - each number is stored as a 16-bit little-endian pair (low byte, high byte)
    assert!(
        validate_value(value),
        "value bigger than 32768 + 8 is invalid"
    );
    let lb: u16 = value % (1 << 8);
    let hb: u16 = value >> 8;
    trace!("  got low byte {:#x} and high byte: {:#x}", lb, hb);
    let byte_pair: (u8, u8) = (lb as u8, hb as u8);
    trace!(
        "  decompose bytes {:?} ({:#x}, {:#x}) from value {} ({:#x}) ",
        byte_pair, byte_pair.0, byte_pair.1, value, value
    );
    byte_pair
}

fn validate_value(val: u16) -> bool {
    val < MAX + 8
}
/// This method takes a provided value validates it and packs it to Data
fn pack_raw_value(v: u16) -> Data {
    match v {
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
    }
}

// TODO: delete it
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

enum ArithmeticOperations {
    Add,
    Multiply,
    Modulo,
    And,
    Or,
    Not,
}
impl fmt::Display for ArithmeticOperations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArithmeticOperations::Modulo => write!(f, "%"),
            ArithmeticOperations::And => write!(f, "&"),
            ArithmeticOperations::Add => write!(f, "+"),
            ArithmeticOperations::Multiply => write!(f, "*"),
            ArithmeticOperations::Or => write!(f, "|"),
            ArithmeticOperations::Not => write!(f, "~"),
        }
    }
}
impl ArithmeticOperations {
    fn get_instruction_name(&self) -> &str {
        match self {
            ArithmeticOperations::Multiply => "mult",
            ArithmeticOperations::Add => "add",
            ArithmeticOperations::And => "and",
            ArithmeticOperations::Or => "or",
            ArithmeticOperations::Not => "not",
            ArithmeticOperations::Modulo => "mod",
        }
    }
}

impl<'b> aux::Commander<'b> for VM {
    fn show_state(&self) {
        trace!("showing VM state to stderr");
        eprintln!("{}", self.get_state());
    }
    fn dump_state(&self, p: &std::path::Path) -> Result<(), std::io::Error> {
        trace!("dumping VM state to {}", p.display());
        std::fs::write(p, self.get_state())
    }
    fn dump_memory(&self, p: &std::path::Path) -> Result<(), std::io::Error> {
        trace!("dumping VM memory to {}", p.display());
        std::fs::write(p, self.memory.as_ref())
    }
    fn record_output(&mut self, p: &std::path::Path) -> Result<(), Box<dyn Error>> {
        if self.is_recording_active() {
            return Err("recording is already enabled to another file".into());
        }
        trace!("starting recording VM output to {}", p.display());

        self.record_output = Some(p.to_path_buf());
        Ok(())
    }
    fn commands_history(&self) -> &[String] {
        trace!(
            "returning {} elements of command history",
            self.commands_history.len()
        );
        self.commands_history.as_ref()
    }
    fn get_replay_commands(&self) -> Vec<String> {
        match &self.replay_commands {
            Some(rc) => rc.clone(),
            None => vec![],
        }
    }
    fn is_recording_active(&self) -> bool {
        self.record_output.is_some()
    }
    fn save_commands_history(&self, dst: &str) -> Result<(), io::Error> {
        trace!("saving commands history to file {}", dst);
        fs::write(dst, self.commands_history().join("\n"))
    }
    /// This function processes the slash commands and return true if the command should be saved to history
    fn process_slash_command(&mut self, cmd: CommandType) -> Result<(), Box<dyn Error>> {
        if let CommandType::Slash(command) = cmd {
            debug!("processing command {}", self.current_command_buf.as_str());
            if command.starts_with("/") {
                trace!("processing slash '/' command");
                match command.to_lowercase().as_str() {
                    "/help" => print_slash_command_help(),
                    "/show_state" => self.show_state(),
                    "/show_history" => {
                        trace!("showing history of commands by demand");
                        eprintln!("{}", self.get_commands_history(0));
                    }
                    "/save_history" => {
                        trace!("saving history of commands by demand");
                        // TODO: Provide an argument to this command
                        const HISTORY_FILE: &str = "history.txt";
                        match self.save_commands_history(HISTORY_FILE) {
                            Ok(_) => eprintln!(
                                "successfully saved commands history to file {}",
                                HISTORY_FILE
                            ),
                            Err(sh_err) => error!(
                                "failed to save commands history to file {} Error: {}",
                                HISTORY_FILE, sh_err
                            ),
                        };
                    }
                    "/show_replay" => {
                        trace!("showing replay commands history");
                        eprintln!("{}", self.get_replay(0));
                    }
                    "/record_output" => {
                        // TODO: Provide an argument to this command
                        trace!("enabling output record by demand");
                        const OUTPUT_FILE: &str = "output.txt";
                        match self.record_output(Into::<PathBuf>::into(OUTPUT_FILE).as_path()) {
                            Ok(()) => eprintln!("output recording started"),
                            Err(e_err) => {
                                error!("failed to start output recording. Error: {}", e_err)
                            }
                        }
                    }
                    "/dump_state" => {
                        trace!("dumping VM state by demand");
                        // TODO: Provide an argument to this command
                        const STATE_FILE: &str = "vm_state.txt";
                        match self.dump_state(Into::<PathBuf>::into(STATE_FILE).as_path()) {
                            Ok(()) => eprintln!("saved VM state to {}", STATE_FILE),
                            Err(st_err) => error!(
                                "failed to save VM state to {} Error: {}",
                                STATE_FILE, st_err
                            ),
                        }
                    }
                    "/dump_memory" => {
                        // TODO: Provide an argument to this command
                        const RAM_FILE: &str = "vm_memory_dump.bin";
                        match self.dump_memory(&Into::<PathBuf>::into(RAM_FILE)) {
                            Ok(()) => eprintln!("saved VM RAM to {}", RAM_FILE),
                            Err(m_err) => {
                                error!("failed to save VM RAM to {} Error: {}", RAM_FILE, m_err)
                            }
                        }
                    }
                    "/solve" => {
                        eprintln!("searching path...");
                        self.maze_analyzer.solve(maze_analyzer::ALLOWED_STEPS);
                    }
                    solve if solve.starts_with("/solve ") => {
                        let steps = solve
                            .strip_prefix("/solve ")
                            .unwrap_or(&format!("{}", maze_analyzer::ALLOWED_STEPS))
                            .to_owned()
                            .parse::<u16>()?;
                        eprintln!("searching path...");
                        self.maze_analyzer.solve(steps);
                    }
                    "/show_path" => {
                        let path = self.maze_analyzer.get_path_back();
                        if path.is_empty() {
                            eprintln!(
                                "no path back was recorded yet. First you need to advance in the maze"
                            );
                        } else {
                            let path_back = path
                                .iter()
                                .rev()
                                .map(|(n, msg, cmd)| {
                                    format!(
                                        "{:03}) {} {}",
                                        n.to_string().green(),
                                        msg.yellow(),
                                        cmd.clone()
                                            .and_then(|c| Some(format!("Command: {}", c).white()))
                                            .unwrap_or("".black())
                                    )
                                })
                                .collect::<Vec<String>>()
                                .join("\n");
                            eprintln!("{}", path_back);
                        }
                    }
                    "/dump_dot" => {
                        let dot_graph_file = PathBuf::from("maze.dot");
                        match self.dump_dot(&dot_graph_file) {
                            Err(st_err) => eprintln!("{}", st_err),
                            Ok(()) => eprintln!("graph has been successfully saved to {}", dot_graph_file.display()),
                        }
                    },
                    user_command => {
                        return Err(format!("unsupported slash command {}", user_command).into());
                    }
                }
                Ok(())
            } else {
                error!("non slash command cannot reach this code!");
                panic!("unsupported command {}", command);
            }
        } else {
            Err("this method supports only commands, which starts with '/' symbol".into())
        }
    }
}

impl VM {
    fn new() -> Self {
        VM {
            halt: false,
            memory: [0; 1 << 16],
            registers: [0; 8],
            stack: VecDeque::new(),
            current_address: Address::default(),
            commands_history: vec![],
            current_command_buf: String::new(),
            record_output: None,
            replay_commands: None,
            replay_buffer: VecDeque::new(),
            output_writer: None,
            maze_analyzer: MazeAnalyzer::new(),
            spin_slash_command: false,
        }
    }
    fn get_state(&self) -> String {
        let mut state = String::new();
        state.push_str("***         Virtual Machine State         ***\n");
        state.push_str(&format!("{}\n", "=".repeat(PRINT_WIDTH)));
        state.push_str(&format!("{:<9}: {}\n", "halt", self.halt));
        state.push_str(&format!("{:<9}: {}\n", "rom size", self.memory.len()));
        state.push_str(&self.get_registers_info(1));
        state.push_str(&self.get_stack_info(1));
        state.push_str(&self.get_replay(1));
        state.push_str(&self.get_commands_history(1));
        state.push_str(&format!("{:<9}: {}\n", "position", self.current_address));
        state.push_str(&format!("{}\n", "_".repeat(PRINT_WIDTH)));
        state.push_str(&format!(
            "{:<9}: {}\n",
            "record out",
            self.record_output
                .clone()
                .map_or("N/A".to_string(), |p| p.display().to_string())
        ));
        state.push_str(&self.maze_analyzer.get_maze_analyzer_state(1));
        state.push_str(&format!("{}\n", "=".repeat(PRINT_WIDTH)));
        state.push_str("^^^        Shortest path back             ^^^\n");
        state.push_str(
            &self
                .maze_analyzer
                .get_path_back()
                .iter()
                .map(|(n, m, c)| {
                    format!(
                        "{:03} {} Command: {}",
                        n,
                        m,
                        c.clone().unwrap_or("N/A".to_string())
                    )
                })
                .collect::<Vec<String>>()
                .join("\n"),
        );
        state.push_str(&format!("\n{}\n", "^".repeat(PRINT_WIDTH)));
        state
    }

    fn dump_dot(&self, dot_graph_file: &Path) -> Result<(), Box<dyn Error>> {
        trace!("dumping graph to {}", dot_graph_file.display());
        let content  = self.maze_analyzer.export_dot_graph()?;
            std::fs::write(dot_graph_file, content)?;
        Ok(())
    }
    fn get_registers_info(&self, indent: usize) -> String {
        let mut registers = String::new();
        let indentation = " ".repeat(indent);
        registers.push_str(&format!("{:<9}:\n", "registers"));
        registers.push_str(&format!(
            "{}{}\n",
            indentation,
            "-".repeat(PRINT_WIDTH - indent)
        ));
        self.registers.iter().enumerate().for_each(|(n, r)| {
            registers.push_str(&format!("{}{}{}: {:<10}\n", indentation, "reg ", n, r))
        });
        registers.push_str(&format!(
            "{}{}\n",
            indentation,
            "-".repeat(PRINT_WIDTH - indent)
        ));
        registers
    }
    fn get_stack_info(&self, indent: usize) -> String {
        let mut stack = String::new();
        let indentation = " ".repeat(indent);
        stack.push_str(&format!(
            "{:<9}  (size: {:3}):\n",
            "stack",
            self.stack.len()
        ));
        stack.push_str(&format!(
            "{}{}\n",
            indentation,
            "+".repeat(PRINT_WIDTH - indent)
        ));
        self.stack
            .iter()
            .enumerate()
            .rev()
            .for_each(|(n, r)| stack.push_str(&format!("{}[{}: {:<10}]\n", indentation, n, r)));
        stack.push_str(&format!(
            "{}{}\n",
            indentation,
            "+".repeat(PRINT_WIDTH - indent)
        ));
        stack
    }
    fn get_commands_history(&self, indent: usize) -> String {
        let mut commands = String::new();
        let indentation = " ".repeat(indent);
        commands.push_str(&format!(
            "{:<9}  (size: {:3}):\n",
            "commands history",
            self.commands_history.len()
        ));
        commands.push_str(&format!(
            "{}{}\n",
            indentation,
            ".".repeat(PRINT_WIDTH - indent)
        ));
        self.commands_history()
            .iter()
            .enumerate()
            .for_each(|(n, r)| commands.push_str(&format!("{}[{}: {:<15}]\n", indentation, n, r)));
        commands.push_str(&format!(
            "{}{}\n",
            indentation,
            ".".repeat(PRINT_WIDTH - indent)
        ));
        commands
    }
    fn get_replay(&self, indent: usize) -> String {
        let mut commands = String::new();
        let indentation = " ".repeat(indent);
        match self.replay_commands {
            Some(ref rc) => {
                commands.push_str(&format!(
                    "{:<9}  (size: {:3}):\n",
                    "replay commands",
                    rc.len()
                ));
                commands.push_str(&format!(
                    "{}{}\n",
                    indentation,
                    ".".repeat(PRINT_WIDTH - indent)
                ));
                rc.iter().enumerate().for_each(|(n, r)| {
                    commands.push_str(&format!("{}[{}: {:<10}]\n", indentation, n, r))
                });
                commands.push_str(&format!(
                    "{}{}\n",
                    indentation,
                    ".".repeat(PRINT_WIDTH - indent)
                ));
            }
            None => commands.push_str(&format!("{:<9}  (size: {:3}):\n", "replay commands", "N/A")),
        }
        commands
    }
    fn new_from_rom(rom: Vec<u8>) -> Self {
        let mut vm = Self::new();
        vm.load_rom(rom);
        vm
    }
    fn new_from_rom_with_options(
        rom: Vec<u8>,
        replay_commands: Option<Vec<String>>,
        record_output: Option<PathBuf>,
    ) -> Self {
        let mut vm = VM {
            replay_commands,
            record_output,
            ..Self::new_from_rom(rom)
        };
        vm.load_replay_buffer();
        vm
    }
    fn load_rom(&mut self, rom: Vec<u8>) {
        debug!("loading program of {} bytes into memory", rom.len());
        for (n, v) in rom.into_iter().enumerate() {
            self.memory[n] = v;
        }
        trace!("loading OK!");
    }
    fn load_replay_buffer(&mut self) {
        if self.replay_commands.is_some() {
            trace!("loading replay commands to the replay buffer");
            self.get_replay_commands()
                .join("\n")
                .chars()
                .for_each(|c| self.replay_buffer.push_back(c));
            //Add trailing new line character to enter the last command
            self.replay_buffer.push_back('\n');
        }
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
        self.grab_output(character, true);
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
        self.set_value_to_register(reg, val);
        self.step_n(3);
    }
    /// This method sets data value of the second argument to the register specified in first
    /// argument
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
        self.do_arithmetic_operation(a, b, c, ArithmeticOperations::Add);
    }

    fn do_arithmetic_on_values(
        &mut self,
        reg: Data,
        v1: Data,
        v2: Option<Data>,
        op: ArithmeticOperations,
    ) {
        // operations add mult mod and or not
        trace!(
            "   storing result of {} operation on {} and {:?} to {}",
            op.get_instruction_name(),
            v1,
            v2,
            reg
        );

        assert!(
            reg.is_register(),
            "first argument value cannot be used as register"
        );
        let val1 = self.unpack_data(v1);
        if let Data::Register(r) = reg {
            let result = match op {
                ArithmeticOperations::Add => {
                    (val1 + self.unpack_data(v2.unwrap_or_else(|| {
                        panic!(
                            "second argumemnt for {} operation is required, but None was provided",
                            op
                        )
                    }))) % MAX
                }
                ArithmeticOperations::Multiply => {
                    (val1 as u64 * self.unpack_data(v2.unwrap_or_else(|| {
                        panic!(
                            "second argumemnt for {} operation is required, but None was provided",
                            op
                        )
                    })) as u64) as u16
                        % MAX
                }
                ArithmeticOperations::And => {
                    (val1 & self.unpack_data(v2.unwrap_or_else(|| {
                        panic!(
                            "second argumemnt for {} operation is required, but None was provided",
                            op
                        )
                    }))) % MAX
                }
                ArithmeticOperations::Or => {
                    (val1 | self.unpack_data(v2.unwrap_or_else(|| {
                        panic!(
                            "second argumemnt for {} operation is required, but None was provided",
                            op
                        )
                    }))) % MAX
                }
                ArithmeticOperations::Not => {
                    trace!(
                        "   performint bitwise negation operation ~ (!) on {} ({:#b})",
                        val1, val1
                    );
                    let result = (!val1) % MAX;
                    trace!("   got negation result {} ({:#b})", result, result);
                    result
                }
                ArithmeticOperations::Modulo => {
                    (val1 % self.unpack_data(v2.unwrap_or_else(|| {
                        panic!(
                            "second argumemnt for {} operation is required, but None was provided",
                            op
                        )
                    }))) % MAX
                }
            };
            trace!(
                "   got arithmetic ops result {} {:#x} {:#b}",
                result, result, result
            );
            self.store_raw_value_to_register(r, result);
        } else {
            panic!("cannot unpack values and register for add operation");
        }
    }

    fn do_arithmetic_operation(
        &mut self,
        a: Address,
        b: Address,
        c: Address,
        op: ArithmeticOperations,
    ) {
        debug!(
            "{} {}: {} {} {}",
            &self.current_address,
            op.get_instruction_name().magenta(),
            &a,
            &b,
            &c
        );
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let value1 = pack_raw_value(self.get_value_from_addr(&b));
        let value2 = pack_raw_value(self.get_value_from_addr(&c));
        self.do_arithmetic_on_values(reg, value1, Some(value2), op);
        self.step_n(4);
    }
    fn mult(&mut self, a: Address, b: Address, c: Address) {
        self.do_arithmetic_operation(a, b, c, ArithmeticOperations::Multiply);
    }
    fn modulo(&mut self, a: Address, b: Address, c: Address) {
        self.do_arithmetic_operation(a, b, c, ArithmeticOperations::Modulo);
    }
    fn and(&mut self, a: Address, b: Address, c: Address) {
        self.do_arithmetic_operation(a, b, c, ArithmeticOperations::And);
    }
    fn or(&mut self, a: Address, b: Address, c: Address) {
        self.do_arithmetic_operation(a, b, c, ArithmeticOperations::Or);
    }
    fn not(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "not".magenta(),
            &a,
            &b
        );
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let value1 = pack_raw_value(self.get_value_from_addr(&b));
        self.do_arithmetic_on_values(reg, value1, None, ArithmeticOperations::Not);
        self.step_n(3);
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

    fn push_to_stack(&mut self, val: u16) {
        trace!("    pushing {} to stack", val);
        self.stack.push_back(val);
    }
    fn pop_from_stack(&mut self) -> u16 {
        let val = self.stack.pop_back().expect("stack is empty");
        trace!("    popped value {} from stack", val);
        val
    }
    fn push(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "push".magenta(), &a);
        // Here used to be a stack bug.
        // IMPORTANT! Befor pushing data to stack the data should be resolved from registers!
        let val = self.get_data_from_addr(a);
        self.push_to_stack(val);
        trace!("pushed value {} to stack", val);
        self.step_n(2);
    }

    fn pop(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "pop".magenta(), &a);
        let val = self.pop_from_stack();
        trace!("popped value {} from stack", val);
        self.set_memory_by_address(a, val);
        self.step_n(2);
    }

    fn set_memory_by_address(&mut self, a: Address, val: u16) {
        trace!(" setting memory by address {} to {}", &a, val);
        let r_data = pack_raw_value(self.get_value_from_addr(&a));
        let v_data = pack_raw_value(val);
        match r_data {
            Data::Register(r) => {
                trace!(
                    " following mem address and setting register {} to value {}",
                    r, val
                );
                self.set_value_to_register(r_data, v_data);
            }
            Data::LiteralValue(_) => {
                let ptr: Ptr = (&a).into();
                let raw_value = self.unpack_data(v_data);
                trace!(
                    "setting literal value {} (orig: {}) to memory address {} (Ptr: {})",
                    raw_value, val, a, ptr
                );
                self.set_memory(ptr, raw_value);
            }
        }
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

    fn gt(&mut self, a: Address, b: Address, c: Address) {
        debug!(
            "{} {}: {} {} {}",
            &self.current_address,
            "gt".magenta(),
            &a,
            &b,
            &c
        );
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let value1 = pack_raw_value(self.get_value_from_addr(&b));
        let value2 = pack_raw_value(self.get_value_from_addr(&c));
        if self.store_greater_than(reg, value1, value2) {
            trace!("successfully stored positive result of comparison");
        } else {
            trace!("successfully stored negative result of comparison");
        }
        self.step_n(4);
    }

    fn store_greater_than(&mut self, reg: Data, v1: Data, v2: Data) -> bool {
        trace!(
            " storing result of gt operation of {} and {} to {}",
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
            if val1 > val2 {
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
    fn call(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "call".magenta(), &a);
        let next_addr = a.next();

        trace!("got address {} and push it to stack", next_addr);
        self.push_to_stack(next_addr.0);
        let pos = Address::new(self.get_data_from_addr(a));
        self.set_position(pos);
    }
    fn ret(&mut self) {
        debug!("{} {}:", &self.current_address, "ret".magenta());
        let addr = self.pop_from_stack();
        self.set_position(Address::new(addr));
    }
    fn rmem(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "rmem".magenta(),
            &a,
            &b
        );
        let val_address = pack_raw_value(self.get_value_from_addr(&b));
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let val = self.get_data_from_addr(Address::new(self.unpack_data(val_address)));
        trace!("got {} and {} after packing", reg, val);
        self.set_value_to_register(reg, pack_raw_value(val));
        self.step_n(3);
    }
    fn wmem(&mut self, a: Address, b: Address) {
        debug!(
            "{} {}: {} {}",
            &self.current_address,
            "wmem".magenta(),
            &a,
            &b
        );
        let val = self.get_data_from_addr(b); //30000
        let val_addr = self.get_data_from_addr(a); //20000
        trace!(" value of b {} value of address from a {}", val, val_addr);
        self.set_memory_by_address(Address::new(val_addr), val);
        self.step_n(3);
    }
    fn get_command_from_buffer(&mut self) -> CommandType {
        let command = self.current_command_buf.clone();
        self.current_command_buf.clear();
        trace!("fetched command {} from buffer", command);
        CommandType::command_type(command.as_str())
    }

    fn process_command(&mut self) -> Result<bool, Box<dyn Error>> {
        // Only next 'enter' should be processed
        let mut do_jump = true; // By default we jump
        if self.spin_slash_command {
            trace!("disabling slash spin, after consuming slash command input from user");
            self.spin_slash_command = false;
            do_jump = false; // but for this last time we skip newline symbol, to not provoke error message.
        }

        let command = self.get_command_from_buffer();
        trace!("processing command {:?}", command);
        match command.clone() {
            CommandType::Slash(cmd) => {
                self.process_slash_command(command)?;
            }
            CommandType::Empty => {
                //Let do nothing
                do_jump = false;
            }
            CommandType::Move(cmd) => {
                cmd.chars().for_each(|c| self.grab_output(c, false));
                self.solver_command_hook(command.clone())?;
                self.store_command_to_history(command);
            }
            _ => {
                self.solver_command_hook(command.clone())?;
                self.store_command_to_history(command);
            }
        }
        // let do_save: bool = self.process_slash_command(&command)?;
        // self.solver_command_hook(&command)?;
        // if do_save {
        //     self.store_command_to_history(command);
        // }
        Ok(do_jump)
    }
    // returns false if command is not stored
    fn store_command_to_history(&mut self, command: CommandType) {
        debug!("storing command {} to command history", command);
        self.commands_history.push(command.into());
        debug!("history size now is {}", self.commands_history.len());
        trace!("after accepting the user command we flush maze analyzer too");
    }
    // returns false if no advance is needed
    fn grab_input(&mut self, c: char) -> bool {
        match c {
            '\n' => {
                let do_jump = self.process_command().unwrap_or(true);
                do_jump
            }
            '/' => {
                if self.current_command_buf.is_empty() {
                    debug!("detected a slash command. Enabling spin");
                    self.spin_slash_command = true;
                }
                self.current_command_buf.push(c);
                !self.spin_slash_command
            }
            c if char_is_printable(c) => {
                self.current_command_buf.push(c);
                !self.spin_slash_command
            }
            _ => {
                warn!("trying to store unprintable character! This should never happen!");
                false
            }
        }
    }
    fn disable_recording(&mut self) {
        trace!("set 'record_output' to None, and thus disabled the output recording");
        self.record_output = None;
    }
    fn grab_output(&mut self, c: char, analyze: bool) {
        if self.is_recording_active() {
            // Init BufWriter if needed
            if self.output_writer.is_none() {
                match File::create(self.record_output.clone().unwrap()) {
                    Ok(f) => {
                        self.output_writer = Some(BufWriter::new(f));
                    }
                    Err(f_err) => {
                        error!(
                            "creation of the output file failed. Error: {} Recording of the output is disabled",
                            f_err
                        );
                        self.disable_recording();
                        return;
                    }
                };
            }
            // Perform write
            if let Some(ref mut bw) = self.output_writer {
                match bw.write(&[c as u8]) {
                    Ok(count) => trace!("wrote {} bytes to the outout buffer", count),
                    Err(buf_e) => {
                        error!(
                            "failed to write character to the output recording buffer. Error: {} Recording stopped",
                            buf_e
                        );
                        self.disable_recording();
                        return;
                    }
                }
                if c == '\n' {
                    self.flush_record_buffer();
                }
            }
        }
        if analyze {
            self.maze_analyzer.push(c);
        }
    }
    fn solver_command_hook(&mut self, command: CommandType) -> Result<(), Box<dyn Error>> {
        if self.maze_analyzer.is_rambling() {
            self.maze_analyzer.dispatch_response(Some(command))?;
            // This will populate the replay buffer
            self.maze_analyzer.ramble(&mut self.replay_buffer);
        }
        Ok(())
    }
    fn solver_response_hook(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.maze_analyzer.is_rambling() || self.maze_analyzer.expect_output() {
            return Err("maze analyzer does not expect an output".into());
        }
        //jump this cycle to re-analyze output
        trace!("need to re-read input");
        let last_command = self
            .commands_history
            .last()
            .map(|l| CommandType::command_type(l));
        self.maze_analyzer.dispatch_response(last_command)?;
        self.maze_analyzer.ramble(&mut self.replay_buffer);
        Ok(())
    }
    /// This function is an implementation of the 'in' operational instruction
    fn read_in(&mut self, a: Address) {
        debug!("{} {}: {}", &self.current_address, "in".magenta(), &a);
        // First we would like to read commands from the replay buffer, if there are any available.
        let c: u8 = match self.replay_buffer.pop_front() {
            Some(replay_char) => {
                eprint!("{}", replay_char.to_string().yellow().underline());
                replay_char as u8
            }
            None => {
                // exit earlier without reading the user input, if the autosolver is working
                // It is needed here, before processing user input.
                // Other invocation is in grab_input/store_command_to_history
                if self
                    .solver_response_hook()
                    .map_err(|e| {
                        debug!("solver response hook returned with error: {}", e);
                        e
                    })
                    .is_ok()
                {
                    return;
                }
                let mut buf: [u8; 1] = [0];

                match io::stdin().read_exact(&mut buf) {
                    Ok(()) => buf[0],
                    Err(e) => {
                        error!("failed to read from stdin. Error: {}", e);
                        panic!("failed on stdin reading");
                    }
                }
            }
        };
        if !self.grab_input(c as char) {
            // Skip the advance when processing slash commands, or something wrong happen.
            return;
        }
        let reg = pack_raw_value(self.get_value_from_addr(&a));
        let val = pack_raw_value(c.into());
        self.set_value_to_register(reg, val);
        self.step_n(2);
    }
    fn main_loop(&mut self) -> Result<u64, Box<dyn Error>> {
        trace!("starting the main loop");
        let mut cycles: u64 = 0;

        loop {
            if self.halt {
                self.show_state();
                break;
            }
            if log_enabled!(Level::Trace) {
                // Debugging
                self.show_state();
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
                    self.gt(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
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

                    self.mult(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                11 => {
                    /*
                                        mod: 11 a b c
                      store into <a> the remainder of <b> divided by <c>
                    */
                    self.modulo(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                12 => {
                    /*
                                        and: 12 a b c
                      stores into <a> the bitwise and of <b> and <c>
                    */
                    self.and(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                13 => {
                    /*
                                        or: 13 a b c
                      stores into <a> the bitwise or of <b> and <c>
                    */
                    self.or(
                        self.current_address.add(1),
                        self.current_address.add(2),
                        self.current_address.add(3),
                    );
                }
                14 => {
                    /*
                                        not: 14 a b
                      stores 15-bit bitwise inverse of <b> in <a>
                    */
                    self.not(self.current_address.add(1), self.current_address.add(2));
                }
                15 => {
                    /*
                                        rmem: 15 a b
                      read memory at address <b> and write it to <a>
                    */
                    self.rmem(self.current_address.add(1), self.current_address.add(2));
                }
                16 => {
                    /*
                                        wmem: 16 a b
                      write the value from <b> into memory at address <a>
                    */
                    self.wmem(self.current_address.add(1), self.current_address.add(2));
                }
                17 => {
                    /*
                        call: 17 a
                      write the address of the next instruction to the stack and jump to <a>
                    */
                    self.call(self.current_address.add(1));
                }
                18 => {
                    /*
                        ret: 18
                      remove the top element from the stack and jump to it; empty stack = halt
                    */
                    self.ret();
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
                    self.read_in(self.current_address.add(1));
                }
                21 => {
                    /*
                        noop: 21
                      no operation

                                unimplemented!("main loop is not implemented yet");
                    */
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
        self.flush_record_buffer();
        Ok(cycles)
    }
    fn flush_record_buffer(&mut self) {
        if let Some(Err(f_err)) = self
            .output_writer
            .as_mut()
            .map(|f: &mut BufWriter<File>| f.flush())
        {
            error!("failed to flush the output record buffer. Error: {}", f_err);
        }
    }
}

pub fn run(config: config::Configuration) -> Result<(), Box<dyn Error>> {
    debug!("received configuration {}", &config);
    if !config.is_valid() {
        return Err("configuration is invalid".into());
    }
    trace!("configuration has been successfully validated");
    let (rom, replay, record_output) = config.rom_replay_record();
    let mut vm = VM::new_from_rom_with_options(rom, replay, record_output);
    let cycles = vm.main_loop()?;
    debug!("VM exited after completing {} cycles", cycles);
    Ok(())
}
