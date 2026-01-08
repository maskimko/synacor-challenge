use clap::Parser;
use log::debug;
use std::ffi::OsString;
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long, default_value = "./challenge.bin")]
    //#[arg(short, long)]
    rom: String,
    #[arg(short = 'R', long)]
    replay: Option<String>,
}

pub fn parse_args() -> Result<Configuration, String> {
    let args = Args::parse();
    debug!("parsed arguments {:?}", args);
    let maybe_replay: Option<OsString> = args.replay.map(OsString::from);
    let rom_file: OsString = args.rom.into();
    Ok(Configuration {
        rom_file,
        replay_file: maybe_replay,
    })
}
#[derive(Debug)]
pub struct Configuration {
    rom_file: OsString,
    replay_file: Option<OsString>,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            rom_file: OsString::from("challenge.bin"),
            replay_file: None,
        }
    }
}
