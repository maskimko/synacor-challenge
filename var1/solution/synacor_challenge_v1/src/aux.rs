use std::error::Error;
use std::path::Path;
use crate::maze_analyzer::CommandType;

pub trait Commander<'b> {
    fn get_replay_commands(&self) -> Vec<String>;
    fn commands_history(&self) -> &[String];
    fn save_commands_history(&self, p: &str) -> Result<(), std::io::Error>;
    fn show_state(&self);
    fn dump_memory(&self, p: &Path) -> Result<(), std::io::Error>;
    fn dump_state(&self, p: &Path) -> Result<(), std::io::Error>;
    fn record_output(&mut self, p: &Path) -> Result<(), Box<dyn Error>>;
    fn is_recording_active(&self) -> bool;
    fn process_slash_command(&mut self, command: CommandType) -> Result<(), Box<dyn Error>>;
}
