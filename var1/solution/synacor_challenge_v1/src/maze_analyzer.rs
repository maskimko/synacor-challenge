use crate::output_parser::{OutputParser, ResponseParts};
use derivative::Derivative;
use log::debug;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::rc::{Rc, Weak};

#[derive(Debug)]
pub struct MazeAnalyzer {
    nodes: HashMap<Rc<Node>, (u16, Option<Rc<Node>>)>,
    completed_nodes: HashSet<Rc<Node>>,
    response_buffer: String,
    first: Option<Rc<Node>>,
    head: Option<Rc<Node>>,
    commands_queue: VecDeque<String>,
    steps_left: u16,
    solution_commands: Option<Vec<String>>,
    commands_counter: u16,
    last_command_num: u16,
}

#[derive(Derivative)]
#[derivative(Debug, PartialEq, Eq, Hash)]
struct Node {
    response: ResponseParts,
    previous: Option<Rc<Node>>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    children: Vec<Weak<Node>>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    // Commands to execute
    edges_to_visit: Vec<String>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    // Executed commands
    visited: Vec<String>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    steps: u16,
}

impl Node {
    fn new(rp: ResponseParts) -> Self {
        Node::new_with_prev(rp, None)
    }
    fn new_with_prev(mut response: ResponseParts, previous: Option<Rc<Node>>) -> Self {
        let edges = Self::get_commands_from_response(&response);
        match previous {
            Some(prev) => {
                let steps = prev.steps + 1;
                let items = prev.response.inventory.clone();
                response.inventory = items;
                let node = Node {
                    edges_to_visit: edges,
                    response,
                    steps,
                    previous: Some(prev),
                    children: vec![],
                    // Commands to execute
                    visited: vec![],
                };
                node
            }
            None => {
                Node {
                    edges_to_visit: edges,
                    response,
                    steps: u16::MAX,
                    previous: None,
                    children: vec![],
                    // Commands to execute
                    visited: vec![],
                }
            }
        }
    }

    fn visit(&mut self, command: &str) {
        self.visited.push(command.to_string());
    }

    fn previous(&self) -> Option<Rc<Self>> {
        self.previous.clone()
    }

    fn get_inventory_from_response(r: &ResponseParts) -> Vec<String> {
        let actions = ["look", "use", "take", "drop"];
        r.inventory
            .iter()
            .flat_map(|i| actions.iter().map(move |a| format!("{} {}", a, i)))
            .collect()
    }
    fn get_exits_from_response(r: &ResponseParts) -> Vec<String> {
        r.exits.iter().map(|ex| format!("go {}", ex)).collect()
    }

    /// First we look, and then try to use the inventory, and then traverse outputs, and as a last
    /// resort use help
    fn get_commands_from_response(r: &ResponseParts) -> Vec<String> {
        [
            &[String::from("help")],
            Self::get_exits_from_response(r).as_slice(),
            Self::get_inventory_from_response(r).as_slice(),
            &["look".to_string()],
        ]
        .concat()
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
        writeln!(f, "   |{:?} steps: {}", self.response, self.steps)?;
        let mut depth = 0;
        let mut previous: Option<Rc<Node>> = self.previous.clone();
        while let Some(prev) = previous {
            depth -= 1;
            writeln!(
                f,
                " {:>03}| {:?} steps: {}",
                depth, prev.response, prev.steps
            )?;
            previous = prev.previous.clone();
        }
        Ok(())
    }
}

impl MazeAnalyzer {
    pub fn new() -> Self {
        MazeAnalyzer {
            nodes: HashMap::new(),
            response_buffer: String::new(),
            first: None,
            head: None,
            commands_queue: VecDeque::new(),
            steps_left: 0,
            solution_commands: None,
            commands_counter: 0,
            last_command_num: 0,
            completed_nodes: HashSet::new(),
        }
    }

    pub fn is_rambling(&self) -> bool {
        self.steps_left > 0
    }
    pub fn expect_output(&mut self) -> bool {
        self.commands_counter != self.last_command_num
    }
    pub fn solution(&self) -> Option<Vec<String>> {
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
                head.visit(command);
                let new_node = Node::new_with_prev(resp_parts, Some(head.clone()));
                let steps = new_node.steps;
                let new_head = Rc::new(new_node);
                self.nodes
                    .insert(new_head.clone(), (steps, Some(head.clone())));
                self.head = Some(new_head);
            }
            None => {
                let mut node = Node::new(resp_parts);
                node.steps = 0;
                let first = Rc::new(node);
                self.nodes.insert(first.clone(), (0, None));
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

    fn validate_steps_left(&self, node: &Node) -> Result<(), String> {
        if node.steps > self.steps_left {
            return Err("exhausted steps".into());
        }
        Ok(())
    }

    /// This function should traverse the maze and find the best route to the exit
    /// Return value shouwl be a vector of the commands to pass the maze
    pub fn search(&mut self, replay_buf: &mut VecDeque<char>) -> Result<Vec<String>, String> {
        if self.head.is_none() {
            return Err("maze analyzer must have a head node".into());
        }
        let node = self.head.clone().unwrap();
        self.validate_steps_left(&node)?;
        if let Some((prev_hash_steps, previous_node)) = self.nodes.get(&node) {
            if previous_node.is_none() || node.steps < *prev_hash_steps {
                self.nodes
                    .insert(node.clone(), (node.steps, previous_node.clone()));
                node.edges_to_visit
                    .iter()
                    .for_each(|cmd| self.commands_queue.push_front(cmd.to_string()));
            }
        }
        // We pop exactly 1 command, because new node will give other commands
        if let Some(cmd) = self.commands_queue.pop_front() {
            cmd.chars().for_each(|c| replay_buf.push_back(c));
            replay_buf.push_back('\n');
            self.last_command_num = self.commands_counter;
        }

        Ok(vec![])
    }

    pub fn solve(&mut self, steps_limit: u16) {
        debug!(
            "started automatic path finding with limit of {}",
            steps_limit
        );
        self.steps_left += steps_limit;
    }
    pub fn ramble(&mut self, replay_buf: &mut VecDeque<char>) {
        if self.expect_output() {
            match self.search(replay_buf) {
                Ok(_) => eprintln!("search finished successfully"),
                Err(e) => eprintln!("search failed: {}", e),
            }
        }
    }
}
