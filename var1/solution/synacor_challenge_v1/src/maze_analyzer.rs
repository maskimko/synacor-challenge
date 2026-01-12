use crate::output_parser::{OutputParser, ResponseParts};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use std::rc::{Rc, Weak};
#[derive(Debug)]
pub struct MazeAnalyzer {
    output_messages: HashMap<Rc<Node>, u16>,
    response_buffer: String,
    first: Option<Rc<Node>>,
    head: Option<Rc<Node>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Node {
    response: ResponseParts,
    next_command: String,
    previous: Option<Rc<Node>>,
    //children: Vec<Weak<Node>>,
    steps: u16,
}

impl Node {
    fn new(rp: ResponseParts, command: &str) -> Self {
        Node::new_with_prev(rp, None, 0, command)
    }
    fn new_with_prev(
        response: ResponseParts,
        previous: Option<Rc<Node>>,
        steps: u16,
        command: &str,
    ) -> Self {
        Node {
            response,
            steps,
            previous,
            next_command: command.to_string(),
        }
    }

    fn previous(&self) -> Option<Rc<Self>> {
        self.previous.clone()
    }

    //fn link_response(&self, rp: ResponseParts) -> Node {
    //    Node {
    //        response: rp,
    //        previous: Some(Rc::new(self)),
    //        steps: self.steps + 1,
    //    }
    //}
    //fn link(&self, n: Node) -> Node {
    //    n.previous = Some(Rc::new(self));
    //    n.steps = self.steps + 1;
    //    n
    //}
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "   |{:?} steps: {} next: {}",
            self.response, self.steps, self.next_command
        )?;
        let mut depth = 0;
        let mut previous: Option<Rc<Node>> = self.previous.clone();
        while let Some(prev) = previous {
            depth -= 1;
            writeln!(
                f,
                " {:>03}| {:?} steps: {}, next: {}",
                depth, prev.response, prev.steps, prev.next_command
            )?;
            previous = prev.previous.clone();
        }
        Ok(())
    }
}

impl MazeAnalyzer {
    pub fn new() -> Self {
        MazeAnalyzer {
            output_messages: HashMap::new(),
            response_buffer: String::new(),
            first: None,
            head: None,
        }
    }

    /// This function adds response from the inner resonse buffer
    pub fn add_response(&mut self, command: &str) -> Result<(), Box<dyn Error>> {
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        match &self.head {
            Some(head) => {
                let steps = head.steps + 1;

                let head = Rc::new(Node::new_with_prev(
                    resp_parts,
                    Some(head.clone()),
                    steps,
                    command,
                ));
                self.output_messages.insert(head.clone(), steps);
                self.head = Some(head);
            }
            None => {
                let first = Rc::new(Node::new(resp_parts, command));
                self.output_messages.insert(first.clone(), 0);
                self.first = Some(first.clone());
                self.head = Some(first)
            }
        }
        self.flush();
        Ok(())
    }

    pub fn push(&mut self, c: char) {
        self.response_buffer.push(c);
    }

    fn flush(&mut self) {
        self.response_buffer.clear();
    }
    //pub fn get_maze_analyzer_state(&self, indent: usize) -> String {
    //    let mut registers = String::new();
    //    let indentation = iter::repeat("  ").take(indent).collect::<String>();
    //    registers.push_str(&format!("{:<9}:\n", "Maze Analyzer"));
    //    registers.push_str(&format!(
    //        "{}{}\n",
    //        indentation,
    //        iter::repeat("#").take(44 - indent).collect::<String>()
    //    ));
    //    self.output_messages.iter().enumerate().for_each(|(n, r)| {
    //        registers.push_str(&format!("{}{:4}", indentation, n));
    //        registers.push_str(&format!(
    //            "{}{}\n",
    //            indentation,
    //            iter::repeat(".").take(40 - indent).collect::<String>()
    //        ));
    //        registers.push_str(&format!("{}{:?}\n", indentation, r));
    //    });
    //    registers.push_str(&format!(
    //        "{}{}\n",
    //        indentation,
    //        iter::repeat("#").take(44 - indent).collect::<String>()
    //    ));
    //    registers
    //}
    pub fn get_maze_analyzer_state(&self, indent: usize) -> String {
        let mut maze = String::new();
        let indentation = " ".repeat(indent);
        maze.push_str(&format!("{:<9}:\n", "Maze Analyzer"));
        maze.push_str(&format!("{}{}\n", indentation, "#".repeat(44 - indent)));

        match &self.head {
            Some(head) => maze.push_str(&format!("{}{}\n", indentation, head)),
            None => maze.push_str("EMPTY"),
        }
        maze.push_str(&format!("{}{}\n", indentation, "#".repeat(44)));
        maze
    }
}
