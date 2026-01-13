use crate::output_parser::{OutputParser, ResponseParts};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;

use std::rc::{Rc, Weak};
use log::debug;

#[derive(Debug)]
pub struct MazeAnalyzer {
    output_messages: HashMap<Rc<Node>, Option<u16>>,
    response_buffer: String,
    first: Option<Rc<Node>>,
    head: Option<Rc<Node>>,
    commands_queue: VecDeque<String>,
    steps_left: u16,
    solution_commands: Option<Vec<String>>,
    commands_counter: u16,
    last_command_num: u16,
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
            commands_queue: VecDeque::new(),
            steps_left: 0,
            solution_commands: None,
            commands_counter: 0,
            last_command_num:0,
        }
    }

    pub fn is_rambling(&self) -> bool {
        self.steps_left > 0
    }
    pub fn expect_output(&mut self) -> bool {
         self.commands_counter != self.last_command_num
    }
    pub fn solution(&self)  -> Option<Vec<String>> {
        self.solution_commands.clone()
    }

    /// This function adds response from the inner resonse buffer
    pub fn add_response(&mut self, command: &str) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() {
            return Ok(());
        }
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
                self.output_messages.insert(head.clone(), None);
                self.head = Some(head);
            }
            None => {
                let first = Rc::new(Node::new(resp_parts, command));
                self.output_messages.insert(first.clone(), None);
                self.first = Some(first.clone());
                self.head = Some(first)
            }
        }
        self.flush();
        self.commands_counter += 1;
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

    fn get_inventory_from_response(r: &ResponseParts) -> Vec<String> {
        let actions = ["look", "use", "take", "drop"];
        r.inventory
            .iter()
            .flat_map(|i| actions.iter().map(move |a| format!("{} {}", a, i)))
            .collect()
        //let mut commands = vec![];
        //for action in ["look", "use","take", "drop"].into_iter() {
        //   for inv_item in r.inventory.iter() {
        //        commands.push(format!("{} {}", action, inv_item));
        //    }
        //}
        //commands
    }
    fn get_exits_from_response(r: &ResponseParts) -> Vec<String> {
        r.exits.iter().map(|ex| format!("go {}", ex)).collect()
    }

    fn get_commands_from_response(r: &ResponseParts) -> Vec<String> {
        [
            &["look".to_string(), String::from("help")],
            Self::get_inventory_from_response(r).as_slice(),
            Self::get_exits_from_response(r).as_slice(),
        ]
        .concat()
    }
    fn get_possible_commands(&self) -> Vec<String> {
        [
            self.head
                .clone()
                .map(|h| {
                    [
                        Self::get_inventory_from_response(&h.response).as_slice(),
                        Self::get_exits_from_response(&h.response).as_slice(),
                    ]
                    .concat()
                })
                .unwrap_or(vec![]),
            ["look".to_string(), String::from("help")].to_vec(),
        ]
        .concat()
    }

    /// This function should traverse the maze and find the best route to the exit
    /// Return value shouwl be a vector of the commands to pass the maze
    pub fn search(
        &mut self,
        replay_buf: &mut VecDeque<char>,
    ) -> Result<Vec<String>, String> {
        let commands = self.get_possible_commands();
        if self.head.is_none() {
            return Err("maze analyzer does not have a head node".into())
        }
            let node = self.head.clone().unwrap();
            let node_steps = node.steps;
            if node_steps > self.steps_left {

                return Err("exhausted steps".into());
            }
            let steps = self.output_messages[&node];
        let should_push_commands : bool = steps.is_none() || node_steps < steps.unwrap_or(u16::MAX);
        if should_push_commands{
                self.output_messages.insert(node, Some(node_steps));
            commands
                .into_iter().rev()
                .for_each(|cmd| self.commands_queue.push_front(cmd));
            // We pop exactly 1 command, because new node will give other commands
            if let Some(cmd) = self.commands_queue.pop_front() {
                cmd.chars().for_each(|c| replay_buf.push_back(c));
                replay_buf.push_back('\n');
                self.last_command_num = self.commands_counter;
            }
            }

        Ok(vec![])
    }

    pub fn solve(&mut self, steps_limit: u16) {
        debug!("started automatic path finding with limit of {}", steps_limit);
       self.steps_left += steps_limit;
    }
    pub fn ramble(
        &mut self,
        replay_buf: &mut VecDeque<char>,
    )  {
        if self.expect_output() {
            match self.search(replay_buf) {
                Ok(_) => eprintln!("search finished successfully"),
                Err(e) => eprintln!("search failed: {}", e),
            }
        }
    }
}
