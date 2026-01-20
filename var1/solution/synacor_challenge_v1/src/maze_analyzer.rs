use crate::output_parser::{OutputParser, ResponseParts};
use clap::{Command, command};
use derivative::Derivative;
use log::{debug, trace, warn};
use std::cell::RefCell;
use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::{cell, fmt};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, Index};
use std::rc::{Rc, Weak};

use crate::dot_graph;
use crate::dot_graph::DotGraphNode;
use colored::Colorize;
use std::hash::DefaultHasher;
use regex::Regex;
use rand::prelude::Rng;
use rand::rng;
use rand::seq::IteratorRandom;

type OptionalNode = Option<Rc<RefCell<Node>>>;

pub const ALLOWED_STEPS: u16 = 100;
#[derive(Debug)]
pub struct MazeAnalyzer {
    // Maps response to the tuple of minimal steps, visits, and origin node if any
    nodes: HashMap<Rc<ResponseParts>, NodeMetadata>,
    // Maps node complete, if all its edges are completed for particular inventory configuration
    // keys is computed inventory hash
    completed_nodes: HashMap<String, HashSet<Rc<ResponseParts>>>,
    last_visited_node: OptionalNode,
    response_buffer: String,
    first: OptionalNode,
    head: OptionalNode,
    commands_queue: VecDeque<String>,
    steps_left: u16,
    solution_commands: Option<Vec<String>>,
    commands_counter: u16,
    last_command_num: u16,
    inventory_needs_update: bool,
    // Tracks the used inventory globally (not per node)
    // Maps inventory name to tuple of uses and looks
    inventory_global: HashMap<String, (u16, u16)>,
    last_node_id: Option<u16>,
    output_is_available: bool
}

#[derive(Debug, Default)]
struct NodeMetadata {
    min_steps: u16,
    visits: u16,
    origin: OptionalNode,
    edges_to_visit: Vec<String>,
    visited_edges: HashMap<String, u16>, // Stores number of visits
    last_visited_edge: Option<String>,
    // Marks edge completed for some set of inventory
    // In other words, with this tools this edge was completed if the node leading to this edge is completed too
    // Keys in inventory computed hash
    completed_edges: HashMap<String, String>,
    response_2_edge: HashMap<Rc<ResponseParts>, String>,
    edge_2_response: HashMap<String, Rc<ResponseParts>>,

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
    come_from: Option<Weak<RefCell<Node>>>,
    // Commands to execute
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    steps: u16,
    id: u16,
}

impl Node {
    fn new(id: u16, response: ResponseParts) -> Self {
        Node {
            response: Rc::new(response),
            steps: u16::MAX,
            previous: None,
            id: id,
            come_from: None,
        }
    }
    fn new_with_prev(id: u16, mut response: ResponseParts, previous: OptionalNode) -> Self {
        match previous {
            Some(prev) => {
                let steps = prev.borrow().steps + 1;
                let items = prev.borrow().response.inventory.clone();
                response.inventory = items;
                let node = Node {
                    steps,
                    previous: Some(prev),
                    ..Self::new(id, response)
                };
                node
            }
            None => Self::new(id, response),
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
            // Marks nodes completed for particular inventory set
            completed_nodes: HashMap::new(),
            inventory_needs_update: false,
            inventory_global: HashMap::new(),
            last_visited_node: None,
            last_node_id: None,
            output_is_available: false,
        }
    }

    pub fn mark_output_available(&mut self) {
        self.output_is_available = true;
    }
    pub fn mark_output_consumed(&mut self) {
        self.output_is_available = false;
    }

    pub fn output_is_available(&self) -> bool {
        self.output_is_available
    }

    fn global_inventory_hash(&self) -> String {
        let mut hasher = DefaultHasher::new();
        let mut sorted_inv: Vec<String> = self.inventory_global.keys().cloned().collect();
        sorted_inv.sort();
        sorted_inv.hash(&mut hasher);
        hasher.finish().to_string()
    }

    pub fn is_rambling(&self) -> bool {
        self.steps_left > 0
    }
    #[deprecated]
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
        let mut new_node = Node {
            response: Rc::new(new_response),
            steps: head.borrow().steps + 1,
            previous: head.borrow().previous().clone(),
            id: head.borrow().id,
            come_from: head.borrow().come_from.clone()
        };
        new_node.steps = head.borrow().steps + 1;
        self.head = Some(Rc::new(RefCell::new(new_node)));
        if head.borrow().previous().is_none() {
            self.first = self.head.clone();
        }
        Ok(())
    }
    fn add_response(&mut self, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() {
            return Ok(());
        }
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;

        // Let's try to visit edge before updating the head



        // Visit edge is not needed here, because no I visit is when issue it to the replay buffer.
        // self.visit_edge(head.clone(), command.to_string().as_str());
        // TODO: visit node here, because then I can register graph nodes even without running the solver.


        if self.head.is_none() {
           // initial response but command exists (replay case)
            self.add_move_response(resp_parts, command)?;
        } else {
        match command.clone() {
            Some(CommandType::InventoryTake(item)) => {
                debug!("taking {} to inventory", item);
                let head = self.head.clone().ok_or("no head")?;
                self.visit_edge(head.clone(), command.clone().unwrap().to_string().as_str());
                let head_response = head.borrow().response();
                let mut inventory = head_response.inventory.clone();
                let mut things = head_response.things_of_interest.clone();
                things.retain(|i| !i.eq(&item));
                inventory.push(item);
                self.update_inventory(head.clone(), inventory, Some(things))?;
                self.set_aux_commands(resp_parts.pretext, command);
                self.inventory_needs_update = true;
            }
            Some(CommandType::InventoryDrop(item)) => {
                debug!("droppoing {} from inventory", item);
                let head = self.head.clone().ok_or("no head")?;
                self.visit_edge(head.clone(), command.clone().unwrap().to_string().as_str());
                let mut inventory = head.borrow().response().inventory.clone();
                inventory.retain(|i| !i.eq(&item));
                self.update_inventory(head.clone(), inventory, None)?;
                self.set_aux_commands(resp_parts.pretext, command);
                self.inventory_needs_update = true;
            }
            Some(CommandType::InventoryUse(item)) => {
                debug!("using {} from inventory", item);
                let head = self.head.clone().ok_or("no head")?;
                self.visit_edge(head, command.clone().unwrap().to_string().as_str());
                (*self.inventory_global.entry(item).or_insert((0, 0))).0 += 1;
                self.inventory_needs_update = true;
                self.set_aux_commands(resp_parts.pretext, command);
            }
            Some(CommandType::InventoryLook(item)) => {
                debug!("using {} from inventory", item);
                let head = self.head.clone().ok_or("no head")?;
                self.visit_edge(head, command.clone().unwrap().to_string().as_str());
                (*self.inventory_global.entry(item).or_insert((0, 0))).1 += 1;
                self.set_aux_commands(resp_parts.pretext, command);
            }
            Some(CommandType::Inventory) => {
                debug!("updating inventory");
                let head = self.head.clone().ok_or("no head")?;
                self.visit_edge(head.clone(), command.clone().unwrap().to_string().as_str());
                self.update_inventory(head.clone(), resp_parts.inventory, None)?;
                self.set_aux_commands(resp_parts.pretext, command);
                self.inventory_needs_update = false;
            }
            None => {
debug!("adding empty command case");
                self.add_move_response(resp_parts, command)?;
            }
             Some(CommandType::Move(cmd)) => {
                 debug!("adding {} to move", cmd);
                self.add_move_response(resp_parts, command)?;
            }
            Some(_) => panic!("never should be called"),
        }}
        let head = self.head.clone().ok_or("no head")?;
        let visits =self.visit_node(head)?;
        debug!("node has {} visits", visits);
        // let mut n_meta = self .nodes .remove(&head.borrow().response()) .ok_or::<String>("no node metadata".into())?;
        // self.nodes .insert(self.head.clone().unwrap().borrow().response(), n_meta);
        self.flush();
        self.commands_counter += 1;
        Ok(())
    }
    fn add_move_response(&mut self, resp_parts: ResponseParts, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
        // debug!("moving {}", destination);
        let is_start_of_graph = self.head.is_none();
        debug!("moving to next node");
        let node_meta_id = self
            .nodes
            .get(&resp_parts)
            .map(|m| m.id)
            .unwrap_or(self.get_node_meta_id());
        let min_steps = self
            .nodes
            .get(&resp_parts)
            .map(|m| m.min_steps)
            .unwrap_or(self.head.clone().map(|h| h.borrow().steps).unwrap_or(0));
        let previous: OptionalNode = self
            .nodes
            .get(&resp_parts)
            .map(|m| m.origin.clone())
            .unwrap_or(self.head.clone());
        let from = self.head.clone().map(|r| Rc::downgrade(&r));
        let new_node = Node {
            previous,
            id: node_meta_id,
            response: Rc::new(resp_parts),
            steps: min_steps,
            come_from: from,
        };
        self.head.clone().map(|h| command.map(|c| self.visit_edge(h.clone(), c.to_string().as_str())));
        // if command.clone().is_some() {
        //     self.visit_edge(self.head.clone().unwrap(), command.clone().unwrap().to_string().as_str());
        // }
        self.head = Some(Rc::new(RefCell::new(new_node))).clone();
        if is_start_of_graph {
            self.first = self.head.clone();
        }
        Ok(())
    }
    fn update_inventory(
        &mut self,
        node: Rc<RefCell<Node>>,
        items: Vec<String>,
        things: Option<Vec<String>>,
    ) -> Result<(), Box<dyn Error>> {
        // Update global inventory
        // Remove items from global inventory, which are not present in the argument
        let mut global_inv = &mut self.inventory_global;
        // Lets not differentiate between 'lit lantern' and 'lantern'
        let allowed: HashSet<&String> = items.iter().collect();
        global_inv.retain(|i, _| allowed.contains(i));
        allowed.into_iter().for_each(|i| {
            global_inv.entry(i.clone()).or_insert((0, 0));
        });

        // Update node inventory
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
        self.replace_head(new_response)?;
        Ok(())
    }
    pub fn get_path_back(&self) -> Vec<(u16, String, Option<String>)> {
        let mut path: Vec<(u16, String, Option<String>)> = vec![];
        let mut current = self.head.clone();
        let mut cmd: Option<String> = None;
        while let Some(node) = current {
            match node.borrow().previous.clone() {
                Some(prev) => {
                    let prev_meta = self
                        .nodes
                        .get(&prev.borrow().response())
                        .expect("previous meta is absent, however the previous node exists");
                    let causing_edge = prev_meta
                        .response_2_edge
                        .get(&node.borrow().response())
                        .cloned();
                    path.push((
                        node.borrow().id,
                        node.borrow().response().message.clone(),
                        cmd.clone(),
                    ));
                    cmd = causing_edge;
                }
                None => {
                    path.push((
                        node.borrow().id,
                        node.borrow().response().message.clone(),
                        cmd.clone(),
                    ));
                }
            }
            current = node.borrow().previous.clone();
        }
        path
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
        // TODO: Merge it with add_response
        match command {
            Some(cmd) => {
                match cmd.clone() {
                    CommandType::Look | CommandType::Help => self.modify_prev_response(Some(cmd)),
                    CommandType::InventoryLook(_)
                    | CommandType::InventoryUse(_)
                    | CommandType::InventoryTake(_)
                    | CommandType::Inventory
                    | CommandType::InventoryDrop(_) => {
                        debug!("dispatching command");
                        self.add_response(Some(cmd))
                    },
                    CommandType::Move(edge) => {
                        debug!("dispatching {} command", edge);
                        self.add_response(Some(cmd))
                    }
                    CommandType::Slash(_) => {
                        Err("slash command  should not be dispatched".into())
                    },
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
    // fn add_response(&mut self, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
    //     // TODO: Combine methods with add_inventory_response
    //     if self.response_buffer.is_empty() {
    //         return Ok(());
    //     }
    //     let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
    //     let is_start_of_graph = self.head.is_none();
    //     let resp_parts = oan.parse()?;
    //     // Assign ID if known
    //     let node_meta_id = self
    //         .nodes
    //         .get(&resp_parts)
    //         .map(|m| m.id)
    //         .unwrap_or(self.get_node_meta_id());
    //     let min_steps = self
    //         .nodes
    //         .get(&resp_parts)
    //         .map(|m| m.min_steps)
    //         .unwrap_or(self.head.clone().map(|h| h.borrow().steps).unwrap_or(0));
    //     let previous: OptionalNode = self
    //         .nodes
    //         .get(&resp_parts)
    //         .map(|m| m.origin.clone())
    //         .unwrap_or(self.head.clone());
    //     let from = self.head.clone().map(|r| Rc::downgrade(&r));
    //     let new_node = Node {
    //         previous,
    //         id: node_meta_id,
    //         response: Rc::new(resp_parts),
    //         steps: min_steps,
    //         come_from: from,
    //     };
    //     self.head = Some(Rc::new(RefCell::new(new_node))).clone();
    //     if is_start_of_graph {
    //         self.first = self.head.clone();
    //     }
    //     self.flush();
    //     self.commands_counter += 1;
    //     Ok(())
    // }

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
        let mapping = &n_meta?.response_2_edge;
        Some(mapping)
    }
    fn get_node_meta_mut(&mut self, node: Rc<RefCell<Node>>) -> Option<&mut NodeMetadata> {
        let n_meta = self.nodes.get_mut(&node.borrow().response())?;
        Some(n_meta)
    }
    fn get_prev_node_meta_mut(&mut self, node: Rc<RefCell<Node>>) -> Option<&mut NodeMetadata> {
        let n_meta = self.get_node_meta_mut(node)?.origin.clone();
        self.get_node_meta_mut(n_meta?)
    }

    fn get_prev_node_resp_map_mut(
        &mut self,
        node: Rc<RefCell<Node>>,
    ) -> Option<&HashMap<Rc<ResponseParts>, String>> {
        let n_meta = self.get_prev_node_meta_mut(node);
        let mapping = &mut n_meta?.response_2_edge;
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
        } else if Self::validate_go_back_command(node.clone(), &"go back".to_string()) {
            Some("go back".to_string())
        }else if node.borrow().response().message.contains("a twisty maze of little passages, all alike") {
            // If we are lost and cannot go back just pick a random one
            let directions = Self::get_exits_from_response(&node.borrow().response());
            let mut rng = rng();
            let pick = rng.random_range(0..directions.len());
            Some(directions[pick].to_string())
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
        // } else if !self.commands_queue.is_empty()  {
        //     Ok(())
        } else {
            // Try to return to previous
            match self.get_command_back_to_previous(node) {
                Some(cmd) => Ok(self.commands_queue.push_front(cmd)),
                None => Err("No commands to visit, and cannot return back".into()),
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

    fn link_nodes(&mut self, node: Rc<RefCell<Node>>, from: cell::Ref<Node>) -> Result<(), String> {
        let resp = node.borrow().response();
        let original_edge = self
            .nodes
            .get(&from.response())
            .ok_or("no node metadata")?
            .last_visited_edge
            .clone()
            .ok_or("no visited edges")?;
        self.nodes
            .entry(from.response())
            .and_modify(|prev_meta| {
                prev_meta
                    .response_2_edge
                    .insert(resp.clone(), original_edge.clone());
                prev_meta
                    .edge_2_response
                    .insert(original_edge, resp.clone());
            });
       Ok(())
    }
    fn link_previous(&mut self, node: Rc<RefCell<Node>>) -> Result<u16, String> {
        let resp = node.borrow().response();
        let prev = node.borrow().previous().ok_or("No previous node")?;
        let from = node.borrow().come_from.clone();
        if let Some(from_nod_ref) = from.map(|f| f.upgrade()).flatten() {
            let weak_link_result  = self.link_nodes(node.clone(), from_nod_ref.borrow());
            trace!("result of linking the node we came from: {:?}", weak_link_result);
        }
        if node.borrow().steps < prev.borrow().steps {
            return Err(
                "won't overwrite previous node, because current node has less steps than previous"
                    .into(),
            );
        }
        self.link_nodes(node, prev.borrow())?;
        // let original_edge = self
        //     .nodes
        //     .get(&prev.borrow().response())
        //     .ok_or("no node metadata")?
        //     .last_visited_edge
        //     .clone()
        //     .ok_or("no visited edges")?;
        // self.nodes
        //     .entry(prev.borrow().response())
        //     .and_modify(|prev_meta| {
        //         prev_meta
        //             .response_2_edge
        //             .insert(resp.clone(), original_edge.clone());
        //         prev_meta
        //             .edge_2_response
        //             .insert(original_edge, resp.clone());
        //     });
        Ok(prev.borrow().steps)
    }
    /// Creates node metadata or increments visits counter
    fn visit_node(&mut self, node: Rc<RefCell<Node>>) -> Result<u16, String> {
        self.nodes
            .entry(node.borrow().response())
            .or_insert(NodeMetadata {
                id: node.borrow().id,
                min_steps: node.borrow().steps,
                origin: node.borrow().previous.clone(),
                edges_to_visit: Self::get_commands_from_response(&node.borrow().response()),
                ..NodeMetadata::default()
            })
            .visits += 1;
        self.last_visited_node = Some(node.clone());
        // link previous
        let link_result = self.link_previous(node.clone());
        trace!("Link result: {:?}", link_result);
        Ok(self
            .nodes
            .get(&node.borrow().response())
            .map(|m| m.visits)
            .unwrap_or(0))
    }

    fn get_node_meta_id(&mut self) -> u16 {
        let last = self.last_node_id.get_or_insert(self.nodes.len() as u16);
        *last += 1;
        *last
    }
    fn is_a_dangerous_edge(
        node: Rc<RefCell<Node>>,
        command: &String,
        prev_command: Option<String>,
    ) -> bool {
        let resp = node.borrow().response();
        // No fear with lit lantern
        if node
            .borrow()
            .response()
            .inventory
            .contains(&"lit lantern".to_string())
        {
            return false;
        }
        // Check for grues
        if resp
            .message
            .contains("likely to be eaten by a")
        {
            return Self::analyse_dangerous_direction(&resp.message, &command).is_ok_and(|danger| danger);
            //return true;
        }
        if resp
            .message
            .contains("become hopelessly lost and are fumbling around")
            && command.contains("forward")
        {
            return true;
        }
        if resp.message.contains("you think you hear a Grue") {
            return prev_command.map(|p| p.eq(command)).unwrap_or(false);
        }

        false
    }

    fn analyse_dangerous_direction(msg: &str, command: &str) -> Result<bool, Box<dyn Error>> {
        if command.contains("continue") {
            return Ok(true);
        }
        let re = Regex::new(r"The (?P<direction>.*) passage appears very dark.*likely to be eaten by a")?;
        let capt = re.captures(msg);
        match capt {
            Some(capt) => {
                let direction = capt.name("direction").ok_or("cannot find dangerous direction from the message")?.as_str();
                Ok(command.contains(direction))
            }
            None => Ok(false),
        }
    }

    pub fn export_dot_graph(&self) -> Result<String, String> {
        let mut graph = dot_graph::DotGraph::new();
        let mut mapping: HashMap<Rc<ResponseParts>, DotGraphNode> = HashMap::new();
        self.nodes.iter().for_each(|(node, meta)| {
            let mut gn = dot_graph::DotGraphNode::new(meta.id, node.title.clone(), node.message.clone());
            gn = graph.add_node(gn);
            mapping.insert(node.clone(), gn);
        });
        self.nodes.iter().for_each(|(node, meta)| {
            meta.response_2_edge.iter().for_each(|(resp, cmd)| {
                let first = mapping.get(node);
                let second = mapping.get(resp);
                if first.is_some() && second.is_some() {
                    graph.add_edge(
                        &first.clone().unwrap(),
                        &second.clone().unwrap(),
                        cmd.clone(),
                    );
                } else {
                    warn!("cannot add to graph None value nodes");
                }
            })
        });
        Ok(graph.dot())
    }

    fn is_looked_or_used_inventory(
        inventory_global: &HashMap<String, (u16, u16)>,
        edge: &str,
    ) -> bool {
        match edge {
            l if edge.starts_with("look ") => inventory_global
                .get(&l.strip_prefix("look ").unwrap().to_string())
                .map(|(_, l)| l)
                .unwrap_or(&0)
                .cmp(&0)
                .is_gt(),
            l if edge.starts_with("use ") => inventory_global
                .get(&l.strip_prefix("use ").unwrap().to_string())
                .map(|(u, _)| u)
                .unwrap_or(&0)
                .cmp(&0)
                .is_gt(),
            _ => false,
        }
    }
    fn get_next_edge(&mut self, node: Rc<RefCell<Node>>, max_times_visited: u16) -> Option<String> {
        // if node.borrow().response().title == "Passage" {
        //     // TODO: delete this line
        //     if node
        //         .borrow()
        //         .response()
        //         .message
        //         .contains("A dark passage leads further west.")
        //     {
        //         warn!("important debug point");
        //     }
        //     if node
        //         .borrow()
        //         .response()
        //         .message
        //         .contains("You are likely to be eaten by a grue.")
        //     {
        //         warn!("continue DANGER debug point");
        //     }
        // }
        let global_inv = &self.inventory_global;
        let to_prev_node = self.get_command_back_to_previous(node.clone());
        let original_edge = node
            .borrow()
            .previous
            .clone()
            .map(|prev| prev.borrow().response())
            .map(|p_resp| self.nodes.get(&p_resp))
            .map(|p| p.map(|op| op.last_visited_edge.clone()))
            .clone()
            .flatten()
            .flatten();
        let mut n_meta = self.nodes.get_mut(&node.borrow().response())?;
        let mut edges_to_visit = n_meta
            .edges_to_visit
            .iter()
            .filter(|e| !n_meta.visited_edges.contains_key(*e))
            .filter(|e| !Self::is_a_dangerous_edge(node.clone(), e, original_edge.clone()))
            .filter(|e| !Self::is_looked_or_used_inventory(global_inv, e))
            .collect::<Vec<_>>();
        while let Some(edge) = edges_to_visit.pop() {
            if edge.contains("forward") {
                warn!("caution!");
            }
            if edge == "use lit lantern" {
                // Just set it with low priority, to prevent using it
                // This prevents flipping 'lantern' to 'lit lantern' and using them in loop
                n_meta.visited_edges.insert(edge.clone(), u16::MAX / 2);
                continue;
            }
            if to_prev_node.as_ref().is_some_and(|p| p.eq(edge)) {
                // We do not want return back, unless there is no other choice. Decreasing priority.
                n_meta.visited_edges.insert(edge.clone(), 2);
                continue;
            }
            return Some(edge.clone());
        }
        self.get_next_edge_least_visited_fallback(node, max_times_visited)
    }

    fn get_next_edge_least_visited_fallback(
        &mut self,
        node: Rc<RefCell<Node>>,
        max_times_visited: u16,
    ) -> Option<String> {
        trace!("all edges have been consumed. Checking second round to find the least consumed");
        let n_meta = self.nodes.get(&node.borrow().response())?;
        // And not visited for particular inventory version
        let last_visited_edge = n_meta.last_visited_edge.clone()?;
        let least_visited: Option<String> = n_meta
            .visited_edges
            .iter()
            .filter(|(k, v)| (**v) < max_times_visited)
            .filter(|(k, _)| matches!(CommandType::command_type(k), CommandType::Move(_)))
            .filter(|(k, _)| !k.as_str().eq(&last_visited_edge))
            .filter(|(k, _)| {
                !Self::is_a_dangerous_edge(node.clone(), k, Some(last_visited_edge.clone()))
            })
            .filter(|(k, _)| self.get_completed_node_by_edge(k, node.clone()).is_none())
            .min_by(|(_key_1, val_1), (_key_2, val_2)| (**val_1).cmp(*val_2))
            .map(|(k, _v)| k.clone());
        least_visited
    }

    fn get_completed_node_by_edge(
        &self,
        edge: &str,
        node: Rc<RefCell<Node>>,
    ) -> Option<Rc<ResponseParts>> {
        let n_meta = self.nodes.get(&node.borrow().response())?;
        let next_resp = n_meta.edge_2_response.get(edge).cloned()?;
        let inv_hash = self.global_inventory_hash();
        let completed_node = self
            .completed_nodes
            .get(&inv_hash)?
            .get(&next_resp)
            .cloned();
        completed_node
    }
    fn visit_edge(&mut self, node: Rc<RefCell<Node>>, command: &str) {
        if let Some(n_meta) = self.nodes.get_mut(&node.borrow().response()) {
            n_meta.edges_to_visit.retain(|c| c != command);
            let visits = n_meta.visited_edges.get(command).unwrap_or(&0) + 1;
            n_meta.visited_edges.insert(command.to_string(), visits);
            n_meta.last_visited_edge = Some(command.to_string());
            //Complete node
            if !n_meta
                .edges_to_visit
                .iter()
                .any(|e| matches!(CommandType::command_type(e), CommandType::Move(_)))
            {
                self.complete_node(node.clone());
            }
        }
    }

    fn previous_is_accessible(&self, node: Rc<RefCell<Node>>) -> bool {
        node.borrow().response().exits.len() > 1
            && self.get_command_back_to_previous(node).is_some()
    }

    fn complete_node(&mut self, node: Rc<RefCell<Node>>) {
        let inv_hash = self.global_inventory_hash();
        let prev_is_completed = node
            .borrow()
            .previous
            .clone()
            .map(|pr| {
                self.completed_nodes
                    .get(&inv_hash)
                    .map(|completed| completed.contains(&pr.borrow().response()))
            })
            .flatten();
        if prev_is_completed.unwrap_or(false) || !self.previous_is_accessible(node.clone()) {
            self.completed_nodes
                .entry(inv_hash)
                .or_insert(HashSet::new())
                .insert(node.borrow().response());
        }
    }

    /// This function should traverse the maze and find the best route to the exit
    /// Return value should be true, if search reached destination
    // TODO: provide destination argument
    pub fn search(&mut self, replay_buf: &mut VecDeque<char>) -> Result<bool, String> {
        if self.head.is_none() {
            return Err("maze analyzer must have a head node".into());
        }
        let node = self.head.clone().unwrap();
        self.validate_steps_left(&node.borrow())?;
        // let node_visits = self.visit_node(node.clone())?;
        // trace!("node visited {} times", node_visits);
        const VISITS_LIMIT_PER_EDGE: u16 = 25;
        self.enqueue_commands(node.clone(), VISITS_LIMIT_PER_EDGE)?;
        // We pop exactly 1 command, because new node will give other commands
        if let Some(cmd) = self.commands_queue.pop_front() {
            // I will visit on the dispatch phase, to allow building graph even without the solver running
            // self.visit_edge(node, &cmd);
            cmd.chars().for_each(|c| replay_buf.push_back(c));
            replay_buf.push_back('\n');
            self.last_command_num = self.commands_counter;
            self.steps_left -= 1; // decrementing each command we issued
        }

        Ok(false)
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
    #[deprecated( note="use search method directly instead")]
    pub fn ramble(&mut self, replay_buf: &mut VecDeque<char>) {
        if self.expect_output() {
            match self.search(replay_buf) {
                Ok(_) => {
                    debug!("search round finished successfully")
                }
                Err(e) => {
                    self.steps_left = 0;
                    self.last_command_num = self.commands_counter;
                    eprintln!("search failed: {}", e)
                }
            }
        }
    }
}
