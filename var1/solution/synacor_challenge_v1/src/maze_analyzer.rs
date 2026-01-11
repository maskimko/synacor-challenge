use std::iter;
use crate::output_parser::{OuputAnalyzer, ResponseParts};
use std::error::Error;
#[derive(Debug)]
pub struct MazeAnalyzer {
    output_messages: Vec<ResponseParts>,
    response_buffer: String
}


impl MazeAnalyzer {
   pub fn new() -> Self {
        MazeAnalyzer { output_messages: vec![], response_buffer: String::new() }
    }

    /// This function adds response from the inner resonse buffer
    pub fn add_response(&mut self) -> Result<(), Box<dyn Error>> {
        let oan : OuputAnalyzer = OuputAnalyzer::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        self.output_messages.push(resp_parts);
        self.flush(); 
        Ok(())
    }

    pub fn push(&mut self, c: char) {
       self.response_buffer.push(c); 
    }

    fn flush(&mut self) {
        self.response_buffer.clear();
    }
    pub fn get_maze_analyzer_state(&self, indent: usize) -> String {
        let mut registers = String::new();
        let indentation = iter::repeat("  ").take(indent).collect::<String>();
        registers.push_str(&format!("{:<9}:\n", "Maze Analyzer"));
        registers.push_str(&format!(
            "{}{}\n",
            indentation,
            iter::repeat("#").take(44 - indent).collect::<String>()
        ));
        self.output_messages.iter().enumerate().for_each(|(n, r)| {
            registers.push_str(&format!("{}{:4}", indentation, n));
        registers.push_str(&format!(
            "{}{}\n",
            indentation,
            iter::repeat(".").take(40 - indent).collect::<String>()
        ));
                registers.push_str(&format!("{}{:?}\n",indentation, r ));
        });
        registers.push_str(&format!(
            "{}{}\n",
            indentation,
            iter::repeat("#").take(44 - indent).collect::<String>()
        ));
        registers
    }
}
