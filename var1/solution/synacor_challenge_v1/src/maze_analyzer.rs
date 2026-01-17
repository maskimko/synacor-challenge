use crate::output_parser::{OutputParser, ResponseParts};
use clap::{command, Command};
use derivative::Derivative;
use log::{debug, trace, warn};
use std::cell::RefCell;
use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::mem::take;
use std::ops::{Deref, Index};
use std::rc::{Rc, Weak};

type OptionalNode = Option<Rc<RefCell<Node>>>;

pub const ALLOWED_STEPS : u16= 100;
#[derive(Debug)]
pub struct MazeAnalyzer {
    // Maps response to the tuple of minimal steps, visits, and origin node if any
    nodes: HashMap<Rc<ResponseParts>, NodeMetadata>,
    completed_nodes: HashSet<Rc<ResponseParts>>,
    response_buffer: String,
    first: OptionalNode,
    head: OptionalNode,
    commands_queue: VecDeque<String>,
    steps_left: u16,
    solution_commands: Option<Vec<String>>,
    commands_counter: u16,
    last_command_num: u16,
    inventory_needs_update: bool
}

#[derive(Debug)]
struct NodeMetadata {
    min_steps: u16,
    visits: u16,
    origin: OptionalNode,
    edges_to_visit: Vec<String>,
    visited_edges: HashMap<String, u16>, // Stores number of visits
    last_visited_edge: Option<String>,
    edge_response: HashMap<Rc<ResponseParts>, String>,
    id: u16,
    auxiliary_commands: HashMap<String, String>,
}

#[derive(Derivative)]
#[derivative(Debug, PartialEq, Eq, Hash)]
struct Node {
    response: Rc<ResponseParts>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    previous: OptionalNode,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    // TODO: use it  or remove it
    children: Vec<Weak<Node>>,
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    // Commands to execute
    steps: u16,
}

impl Node {
    fn new(response: ResponseParts) -> Self {
        Node {
            response: Rc::new(response),
            steps: u16::MAX,
            previous: None,
            children: vec![],
        }
    }
    fn new_with_prev(mut response: ResponseParts, previous: OptionalNode) -> Self {
        match previous {
            Some(prev) => {
                let steps = prev.borrow().steps + 1;
                let items = prev.borrow().response.inventory.clone();
                response.inventory = items;
                let node = Node {
                    steps,
                    previous: Some(prev),
                    ..Self::new(response)
                };
                node
            }
            None => Self::new(response),
        }
    }

    fn response(&self) -> Rc<ResponseParts> {
        self.response.clone()
    }

    fn previous(&self) -> OptionalNode {
        self.previous.clone()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "   |{:?} steps: {}", self.response, self.steps)?;
        let mut depth = 0;
        let mut previous: OptionalNode = self.previous.clone();
        while let Some(prev) = previous {
            depth -= 1;
            writeln!(
                f,
                " {:>03}| {:?} steps: {}",
                depth,
                prev.borrow().response,
                prev.borrow().steps
            )?;
            previous = prev.borrow().previous();
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum CommandType {
    Look,
    Help,
    Inventory,
    InventoryTake(String),
    InventoryDrop(String),
    InventoryLook(String),
    InventoryUse(String),
    Move(String),
    Slash(String),
    Empty,
}
impl CommandType {
    pub fn command_type(cmd: &str) -> CommandType {
        match cmd {
            "look" => CommandType::Look,
            "help" => CommandType::Help,
            "inv" => CommandType::Inventory,
            c if c.starts_with("take ") => {
                CommandType::InventoryTake(c.to_string()[5..].to_string())
            }
            c if c.starts_with("look ") => {
                CommandType::InventoryLook(c.to_string()[5..].to_string())
            }
            c if c.starts_with("use ") => CommandType::InventoryUse(c.to_string()[4..].to_string()),
            c if c.starts_with("drop ") => {
                CommandType::InventoryDrop(c.to_string()[5..].to_string())
            }
            c if c.starts_with("/") => CommandType::Slash(c.to_string()),
            c if c.trim().is_empty() => CommandType::Empty,
            c => CommandType::Move(c.to_string()),
        }
    }
}

impl From<CommandType> for String {
    fn from(cmd: CommandType) -> String {
        format!("{}", cmd)
    }
}
impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Look => write!(f, "look"),
            CommandType::Help => write!(f, "help"),
            CommandType::Inventory => write!(f, "inv"),
            CommandType::InventoryTake(c) => write!(f, "take {}", c),
            CommandType::InventoryDrop(c) => write!(f, "drop {}", c),
            CommandType::InventoryLook(c) => write!(f, "look {}", c),
            CommandType::InventoryUse(c) => write!(f, "use {}", c),
            CommandType::Move(c) => write!(f, "{}", c),
            CommandType::Slash(c) => write!(f, "{}", c),
            CommandType::Empty => write!(f, "[EMPTY (user pressed just enter)]"),
        }
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
            inventory_needs_update: false
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

    fn set_aux_commands(&mut self, output: String, command: Option<CommandType>) -> Option<()> {
        let resp = self.head.clone()?.borrow().response();
        let mut n_meta = self.nodes.remove(&resp)?;
        n_meta
            .auxiliary_commands
            .insert(command?.to_string(), output);
        // And return it back
        self.nodes.insert(resp, n_meta);
        Some(())
    }
    fn replace_head(&mut self, new_response: ResponseParts) -> Result<(), Box<dyn Error>> {
        let head = self.head.clone().ok_or("no head")?;
        // Replace head
        let mut new_node =
        Node{response: Rc::new(new_response), steps: head.borrow().steps+1, previous: head.borrow().previous().clone(), children: head.borrow().children.clone()};
        new_node.steps = head.borrow().steps + 1;
        self.head = Some(Rc::new(RefCell::new(new_node)));
        if head.borrow().previous().is_none() {
            self.first = self.head.clone();
        }
        Ok(())
    }
    fn add_inventory_response(&mut self, command: CommandType) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() {
            return Ok(());
        }
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        let head = self.head.clone().ok_or("no head")?;
        let head_response = head.borrow().response();
        // Visit edge
        self.visit_edge(head.clone(), command.to_string().as_str());

        let mut n_meta = self
            .nodes
            .remove(&head.borrow().response())
            .ok_or::<String>("no node metadata".into())?;

        match command.clone() {
            CommandType::InventoryTake(item) => {
                debug!("taking {} to inventory", item);
                let mut inventory = head_response.inventory.clone();
                let mut things = head_response.things_of_interest.clone();
                things.retain(|i| !i.eq(&item));
                inventory.push(item);
                self.update_inventory(head.clone(), inventory, Some(things))?;

                self.inventory_needs_update = true;
            }
            CommandType::InventoryDrop(item) => {
                debug!("droppoing {} from inventory", item);
                let mut inventory = head.borrow().response().inventory.clone();
                inventory.retain(|i| !i.eq(&item));
                self.update_inventory(head.clone(), inventory,None)?;
                self.inventory_needs_update = true;
            },
            CommandType::InventoryUse(item) => {
                debug!("using {} from inventory", item);
                self.inventory_needs_update = true;
            },
            CommandType::InventoryLook(item) => {
                debug!("using {} from inventory", item);
            },
            CommandType::Inventory => {
                debug!("updating inventory");
                let mut inventory = head.borrow().response().inventory.clone();
                self.update_inventory(head.clone(), resp_parts.inventory,None)?;
                self.inventory_needs_update = false;

            }
            _ => panic!("never should be called"),
        }
        self.nodes .insert(self.head.clone().unwrap().borrow().response(), n_meta);
        self.set_aux_commands(resp_parts.pretext, Some(command));
        self.flush();
        self.commands_counter += 1;
        Ok(())
    }
    fn update_inventory(&mut self, node: Rc<RefCell<Node>>, items: Vec<String>, things: Option<Vec<String>>) -> Result<(), Box<dyn Error>> {

        let head_response = node.borrow().response();
        let new_response: ResponseParts = ResponseParts {
            inventory: items,
            things_of_interest: things.unwrap_or(head_response.things_of_interest.clone()),
            pretext: head_response.pretext.clone(),
            message: head_response.message.clone(),
            title: head_response.title.clone(),
            exits: head_response.exits.clone(),
            dont_understand: head_response.dont_understand.clone(),
        };
        // // Replace head
        // let mut new_node =
        //     Node::new_with_prev(new_response, node.borrow().previous.clone());
        // new_node.steps = node.borrow().steps + 1;
        // self.head = Some(Rc::new(RefCell::new(new_node)));
        // if node.borrow().previous().is_none() {
        //     self.first = self.head.clone();
        // }
        self.replace_head(new_response)?;
        Ok(())
    }
    fn modify_prev_response(&mut self, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() || command.is_none() {
            return Ok(());
        }
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        self.flush();
        self.commands_counter += 1;
        self.visit_edge(
            self.head.clone().ok_or::<String>("no head".into())?.clone(),
            command
                .clone()
                .ok_or::<String>("no command".into())?
                .to_string()
                .as_str(),
        );
        self.set_aux_commands(resp_parts.pretext, command)
            .ok_or("failed to insert command".into())
    }
    pub fn dispatch_response(
        &mut self,
        command: Option<CommandType>,
    ) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() {
            return Ok(());
        }
        match command {
            Some(cmd) => {
                match cmd.clone() {
                    CommandType::Look
                    | CommandType::Help
|                    CommandType::InventoryLook(_)=> self.modify_prev_response(Some(cmd)),
                    | CommandType::InventoryUse(_)
                    | CommandType::InventoryTake(_)
                    | CommandType::Inventory
                    | CommandType::InventoryDrop(_) =>  self.add_inventory_response(cmd),
                    CommandType::Move(edge) => {
                        debug!("dispatching {} command", edge);
                        self.add_response(Some(cmd))
                    }
                    CommandType::Slash(_) => Err("slash command  should not be dispatched".into()),
                    CommandType::Empty => {
                        debug!("user issued empty command. No operations performed");
                        // Tolerating
                        Ok(())
                    }
                }
            }
            None => {
                //This usually means that this user's first command was /solve
                debug!("dispatching to save initial response");
                self.add_response(None)
            }
        }
    }
    /// This function adds response from the inner resonse buffer
    fn add_response(&mut self, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
        // TODO: Combine methods with add_inventory_response
        if self.response_buffer.is_empty() {
            return Ok(());
        }
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        match self.head.clone() {
            Some(head) => {
                // TODO: Improve with match statement

                self.visit_edge(head.clone(), command.unwrap().to_string().as_str());
                // with look, help, and inv commands (take, use, drop, look) let's not create a new node
                let new_node = Node::new_with_prev(resp_parts, Some(head.clone()));
                let steps = new_node.steps;
                // Let's not insert node here, and keep this data structure for tracking visited nodes during the search
                // self.nodes .insert(new_node.response(), (steps, Some(head.clone())));
                self.head = Some(Rc::new(RefCell::new(new_node)))
            }
            None => {
                let mut node = Node::new(resp_parts);
                node.steps = 0;
                // Let's not insert node here, and keep this data structure for tracking visited nodes during the search
                // self.nodes.insert(node.response(), (0, None));
                let first = Rc::new(RefCell::new(node));
                self.first = Some(first.clone());
                self.head = Some(first);
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
    pub fn get_maze_analyzer_state(&self, indent: usize) -> String {
        let mut maze = String::new();
        let indentation = " ".repeat(indent);
        maze.push_str(&format!("{:<9}:\n", "Maze Analyzer"));
        maze.push_str(&format!("{}{}\n", indentation, "#".repeat(44 - indent)));

        match &self.head {
            Some(head) => maze.push_str(&format!("{}{}\n", indentation, head.borrow())),
            None => maze.push_str("EMPTY"),
        }
        maze.push_str(&format!("{}{}\n", indentation, "#".repeat(44)));
        maze
    }

    fn get_things_of_interest_from_response(r: &ResponseParts) -> Vec<String> {
        let actions = ["look", "take"];
        let things_commands = r
            .things_of_interest
            .iter()
            .flat_map(|i| actions.iter().map(move |a| format!("{} {}", a, i)))
            .collect();
        things_commands
    }
    fn get_inventory_from_response(r: &ResponseParts) -> Vec<String> {
        let actions = [
            "use", // "drop",  // let's not drop things
            // "take",
            "look",
        ];
        let inv_commands = r
            .inventory
            .iter()
            .flat_map(|i| actions.iter().map(move |a| format!("{} {}", a, i)))
            .collect();
        inv_commands
    }
    fn get_exits_from_response(r: &ResponseParts) -> Vec<String> {
        r.exits.iter().map(|ex| format!("go {}", ex)).collect()
    }

    fn get_commands_from_response(r: &ResponseParts) -> Vec<String> {
        [
            // Lets try without look and help
            // &["look".to_string(), String::from("help")],
            Self::get_things_of_interest_from_response(r).as_slice(),
            // &[ "inv".to_string()],
            Self::get_inventory_from_response(r).as_slice(),
            Self::get_exits_from_response(r).as_slice(),
        ]
        // Revers it so .pop() will take the first value
        .concat()
        .iter()
        .rev()
        .cloned()
        .collect()
    }

    /// Validate how many steps left
    fn validate_steps_left(&self, node: &Node) -> Result<(), String> {
        if self.steps_left == 0 {
            return Err("exhausted steps".into());
        }
        Ok(())
    }

    fn get_node_meta(&self, node: Rc<RefCell<Node>>) -> Option<&NodeMetadata> {
        let n_meta = self.nodes.get(&node.borrow().response())?;
        Some(n_meta)
    }
    fn get_prev_node_meta(&self, node: Rc<RefCell<Node>>) -> Option<&NodeMetadata> {
        let n_meta = self.get_node_meta(node)?.origin.clone();
        self.get_node_meta(n_meta?)
    }

    fn get_prev_node_resp_map(
        &self,
        node: Rc<RefCell<Node>>,
    ) -> Option<&HashMap<Rc<ResponseParts>, String>> {
        let n_meta = self.get_prev_node_meta(node);
        let mapping = &n_meta?.edge_response;
        Some(mapping)
    }
    fn get_node_meta_mut<'a>(
        &'a mut self,
        node: Rc<RefCell<Node>>,
    ) -> Option<&'a mut NodeMetadata> {
        let n_meta = self.nodes.get_mut(&node.borrow().response())?;
        Some(n_meta)
    }
    fn get_prev_node_meta_mut<'a>(
        &'a mut self,
        node: Rc<RefCell<Node>>,
    ) -> Option<&'a mut NodeMetadata> {
        let n_meta = self.get_node_meta_mut(node)?.origin.clone();
        self.get_node_meta_mut(n_meta?)
    }

    fn get_prev_node_resp_map_mut<'a>(
        &'a mut self,
        node: Rc<RefCell<Node>>,
    ) -> Option<&'a HashMap<Rc<ResponseParts>, String>> {
        let n_meta = self.get_prev_node_meta_mut(node);
        let mapping = &mut n_meta?.edge_response;
        Some(mapping)
    }
    fn validate_go_back_command(node: Rc<RefCell<Node>>, cmd: &String) -> bool {
        Self::get_exits_from_response(&node.borrow().response()).contains(cmd)
    }
    fn get_command_back_to_previous(&self, node: Rc<RefCell<Node>>) -> Option<String> {
        let prev_mapping = self.get_prev_node_resp_map(node.clone())?;
        let cause_command = prev_mapping.get(&node.borrow().response())?.to_string();
        let oposite_command = match cause_command.as_str() {
            "go north" => "go south".to_string(),
            "go south" => "go north".to_string(),
            "go west" => "go east".to_string(),
            "go east" => "go west".to_string(),
            cmd => cmd.to_string(),
        };
        if Self::validate_go_back_command(node.clone(), &oposite_command) {
            Some(oposite_command)
        } else if Self::validate_go_back_command(node.clone(), &"go back".to_string())   {
            Some("go back".to_string())
        } else {
            warn!(
                "Cannot validate opposite command: {}. So there is no path to return? Trying it anyway...",
                cause_command
            );
            None
        }
    }
    /// This method returns new edge to visit. It checks if the edge was not visited more than 'visits_limit' times.
    fn enqueue_commands(
        &mut self,
        node: Rc<RefCell<Node>>,
        visits_limit: u16,
    ) -> Result<(), String> {
        // Maze analyzer should compare the steps value of the previous node with the minimal value from the hash map.
        // If it is greater, than it means that it was not an optimal way to go. And commands should not be enqueued the second time.

        if self.inventory_needs_update {
            self.commands_queue.push_front("inv".to_string());
            Ok(())
        } else if let Some(cmd) = self.get_next_edge(node.clone(), visits_limit) {
            self.commands_queue.push_front(cmd);
            Ok(())
        } else if !self.commands_queue.is_empty()  {
            Ok(())
        } else {

            // Try to return to previous
            match self.get_command_back_to_previous(node) {
                Some(cmd) => Ok(self.commands_queue.push_front(cmd)),
                None => {
                    Err("No commands to visit, and cannot return back".into()) },
            }
        }
    }
    // returns times node visited and min steps to visit it
    fn times_was_visited(&self, node: Rc<RefCell<Node>>) -> (u16, u16) {
        if let Some(n_meta) = self.nodes.get(&node.borrow().response()) {
            return (n_meta.visits, n_meta.min_steps);
        } else {
            (0, u16::MAX)
        }
    }

    fn link_previous(&mut self, node: Rc<RefCell<Node>>) -> Option<()> {
        let resp = node.borrow().response();
        let prev = node.borrow().previous()?;
        let mut prev_meta = self.nodes.remove(&prev.borrow().response())?;
        prev_meta
            .edge_response
            .insert(resp.clone(), prev_meta.last_visited_edge.clone()?);
        self.nodes.insert(prev.borrow().response(), prev_meta);
        Some(())
    }
    /// Creates node metadata or increments visits counter
    fn visit_node(&mut self, node: Rc<RefCell<Node>>) {
        if let Some(mut n_meta) = self.nodes.get_mut(&node.borrow().response()) {
            n_meta.visits += 1;
            // self.nodes.insert(
            //     node.borrow().response(),
            //     NodeMetadata {
            //         min_steps: min(n_meta.min_steps, node.borrow().steps),
            //         visits: n_meta.visits + 1,
            //         origin: n_meta.origin.clone(),
            //         edges_to_visit: n_meta.edges_to_visit.clone(),
            //         visited_edges: n_meta.visited_edges.clone(),
            //         last_visited_edge: n_meta.last_visited_edge.clone(),
            //         edge_response: n_meta.edge_response.clone(),
            //         id: n_meta.id,
            //         auxiliary_commands: n_meta.auxiliary_commands.clone(),
            //     },
            // );
        } else {
            self.nodes.insert(
                node.borrow().response(),
                NodeMetadata {
                    min_steps: node.borrow().steps,
                    visits: 1,
                    origin: node.borrow().previous.clone(),
                    edges_to_visit: Self::get_commands_from_response(&node.borrow().response()),
                    visited_edges: HashMap::new(),
                    last_visited_edge: None,
                    edge_response: HashMap::new(),
                    id: self.get_node_meta_id(),
                    auxiliary_commands: HashMap::new(),
                },
            );
        }
        // link previous
        self.link_previous(node);
    }

    fn get_node_meta_id(&self) -> u16 {
        self.nodes.len() as u16 + 1
    }
    fn is_a_dangerous_edge(node: Rc<RefCell<Node>>, command: &String) -> bool {
        let resp = node.borrow().response();
        // No fear with lit lantern
        if node.borrow().response().inventory.contains(&"lit lantern".to_string()) {
            return false
        }
           if resp.message.contains("You are likely to be eaten by a grue.") ||resp.message.contains("have become hopelessly lost")  {
           if  command.contains("continue") {
              return true;
           }
       }
        false
    }
    fn get_next_edge(&mut self, node: Rc<RefCell<Node>>, max_times_visited: u16) -> Option<String> {
        if node.borrow().response().title == "Passage" {
            // TODO: delete this line
            warn!("important debug point")
        }
        let mut n_meta= self .nodes .get_mut(&node.borrow().response())?;
        while let Some(edge) = n_meta
            .edges_to_visit
            .pop()
        {
            if !n_meta
                .visited_edges
                .contains_key(&edge) && !Self::is_a_dangerous_edge(node.clone(), &edge)
            {
                return Some(edge);
            }
        }
        trace!("all edges have been consumed. Checking second round to find the least consumed");
        let last_visited_edge = n_meta.last_visited_edge.clone()?;
        let least_visited: Option<String> = n_meta
            .visited_edges
            .iter()
            .filter(|(k, v)| {
                (**v) < max_times_visited
                    && matches!(CommandType::command_type(k), CommandType::Move(_))
                && !k.as_str().eq(&last_visited_edge)
                && !Self::is_a_dangerous_edge(node.clone(), k)
            })
            .min_by(|(_key_1, val_1), (_key_2, val_2)| (**val_1).cmp(*val_2))
            .map(|(k, _v)| k.clone());
        least_visited
    }
    fn visit_edge(&mut self, node: Rc<RefCell<Node>>, command: &str) {
        if let Some(n_meta) = self.nodes.get_mut(&node.borrow().response()) {
            n_meta.edges_to_visit.retain(|c| c != command);
            let visits = n_meta.visited_edges.get(command).unwrap_or(&0) + 1;
            n_meta.visited_edges.insert(command.to_string(), visits);
            n_meta.last_visited_edge = Some(command.to_string());
            if n_meta.edges_to_visit.is_empty() {
                self.completed_nodes.insert(node.borrow().response());
            }
        }
    }

    /// This function should traverse the maze and find the best route to the exit
    /// Return value should be a vector of the commands to pass the maze
    pub fn search(&mut self, replay_buf: &mut VecDeque<char>) -> Result<Vec<String>, String> {
        if self.head.is_none() {
            return Err("maze analyzer must have a head node".into());
        }
        let node = self.head.clone().unwrap();
        self.validate_steps_left(&node.borrow())?;
        self.visit_node(node.clone());
        const VISITS_LIMIT_PER_EDGE: u16 = 5;
        self.enqueue_commands(node, VISITS_LIMIT_PER_EDGE)?;
        // We pop exactly 1 command, because new node will give other commands
        if let Some(cmd) = self.commands_queue.pop_front() {
            cmd.chars().for_each(|c| replay_buf.push_back(c));
            replay_buf.push_back('\n');
            self.last_command_num = self.commands_counter;
            self.steps_left -= 1; // decrementing each command we issued
        }

        Ok(vec![])
    }

    pub fn solve(&mut self, steps_limit: u16) {
        debug!(
            "started automatic path finding with limit of {}",
            steps_limit
        );
        // This enables rambling / serching path
        self.steps_left += steps_limit;
        //  self.commands_counter += 1; //To expect output
    }
    pub fn ramble(&mut self, replay_buf: &mut VecDeque<char>) {
        if self.expect_output() {
            match self.search(replay_buf) {
                Ok(_) => {
                    debug!("search round finished successfully")
                },
                Err(e) => {
                    self.steps_left = self.commands_counter;
                    eprintln!("search failed: {}", e)
                },
            }
        }
    }
}
