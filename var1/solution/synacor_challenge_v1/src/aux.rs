use std::slice::Iter;
use std::path::Path;

pub trait Commander<'a> {
    fn get_replay_commands(&self) -> Iter<&'a str >;
    fn commands_history(&self) -> Vec<String>;
    fn save_commands_history(&self, p : &Path) -> Result<(), std::io::Error> ;
    fn show_state(&self);
    fn dump_memory(&self, p: &Path) -> Result<(), std::io::Error>;
    fn dump_state(&self, p: &Path)  -> Result<(), std::io::Error>;
    fn record_output(&self, p: &Path)  -> Result<(), std::io::Error>;
    fn is_recording_active(&self) -> bool;
}
