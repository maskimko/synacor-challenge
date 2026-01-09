use clap::Parser;
use log::{debug, trace, warn};
use std::error::Error;
use std::fmt::{self, Formatter};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long, default_value = "./challenge.bin")]
    //#[arg(short, long)]
    rom: String,
    #[arg(short = 'R', long)]
    replay: Option<String>,
}

pub fn parse_args() -> Result<Configuration, Box<dyn Error>> {
    let args = Args::parse();
    debug!("parsed arguments {:?}", args);
    let maybe_replay: Option<OsString> = args.replay.map(OsString::from);
    let rom_file: OsString = args.rom.into();
    let mut conf = Configuration::new(rom_file.into(), maybe_replay.map(PathBuf::from));
    conf.read_in()?;
    Ok(conf)
}
#[derive(Debug)]
pub struct Configuration {
    rom_file: PathBuf,
    replay_file: Option<PathBuf>,
    rom: Vec<u8>,
    replay_commands: Vec<String>,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            rom_file: PathBuf::from("challenge.bin"),
            replay_file: None,
            rom: vec![],
            replay_commands: vec![],
        }
    }
}

impl fmt::Display for Configuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_> ) -> fmt::Result {
        if self.replay_file.is_some() {
        write!(f,"Configuration:\n\tROM file: {}\n\treplay file: {}\n\tROM size: {} bytes\n\treplay cmds. qty.: {}", &self.rom_file.display(), &self.replay_file.as_ref().map(|f| f.display()).unwrap(), &self.rom.len(), &self.replay_commands.len())
        } else {
        write!(f,"Configuration:\n\tROM file: {}\n\treplay file: N/A\n\tROM size: {} bytes\n\treplay cmds. qty.: 0", &self.rom_file.display(), &self.rom.len())

        }
    }
}

impl Configuration {
    fn new(rom_file: PathBuf, replay_file: Option<PathBuf>) -> Self {
        Configuration {
            rom_file: rom_file,
            replay_file: replay_file,
            rom: vec![],
            replay_commands: vec![],
        }
    }
    pub fn read_in(&mut self) -> Result<(usize, usize), Box<dyn Error>> {
        let mut rom_file = File::open(&self.rom_file)?;
        let mut buf: Vec<u8> = Vec::with_capacity(60 * 1024); // The size of the chanllenge binary
        // is roughly 60kb
        let was_read = rom_file.read_to_end(&mut buf)?;
        trace!(
            "successfully read {} bytes from {}",
            was_read,
            &self.rom_file.display()
        );
        self.rom = buf;
        let mut commands_read = 0;
        if let Some(replay_file) = &self.replay_file {
            let rep_f = File::open(replay_file)?;
            let reader = BufReader::new(rep_f);
            let mut errors = vec![];
            // probably it is better to use here .partition(Result::is_ok)
            let lines: Vec<String> = reader
                .lines()
                .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
                .collect();
            commands_read = lines.len();
            if !errors.is_empty() {
                warn!(
                    "during the replay commands file read there errors occurred {:?}",
                    errors
                );
            }
            trace!(
                "successfully read {} lines from {}",
                commands_read,
                replay_file.display()
            );
            self.replay_commands = lines;
        }
        Ok((was_read, commands_read))
    }
    pub fn is_valid(&self) -> bool {
        // IMPROVEMENT_IDEA: probably to add support of reading bytes from stdin
        let rom_file_is_present = match fs::exists(&self.rom_file) {
            Ok(exists) => exists,
            Err(e) => {
                warn!("cannot check existance of the ROM file. Error: {}", e);
                false
            }
        };
        if self.rom.is_empty() {
            warn!("ROM is empty. Probably you need to load the memory from the file first");
        }
        !self.rom.is_empty() && rom_file_is_present
    }

    pub fn rom(&self) -> Vec<u8> {
        self.rom.clone()
    }

    pub fn replay(&self) -> Vec<String> {
        self.replay_commands.clone()
    }

    pub fn rom_n_replay(self) -> (Vec<u8>, Vec<String>) {
        (self.rom, self.replay_commands) 
    }
}
