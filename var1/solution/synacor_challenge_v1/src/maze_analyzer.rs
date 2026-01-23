use crate::output_parser::{OutputParser, ResponseParts};
use clap::{Command, command};
use derivative::Derivative;
use log::{debug, trace, warn};
use std::cell::RefCell;
use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt::Formatter;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, Index};
use std::rc::{Rc, Weak};
use std::{cell, fmt};

use crate::dot_graph;
use crate::dot_graph::DotGraphNode;
use colored::Colorize;
use rand::prelude::Rng;
use rand::rng;
use rand::seq::IteratorRandom;
use regex::Regex;
use std::hash::DefaultHasher;

// type OptionalNode = Option<Rc<RefCell<Node>>>;
type RID = Rc<ResponseId>;
type ORID = Option<RID>;
type OWID = Option<Weak<ResponseId>>;

pub const ALLOWED_STEPS: u16 = 100;
#[derive(Debug)]
pub struct MazeAnalyzer {
    // Maps response to the tuple of minimal steps, visits, and origin node if any
    nodes: HashMap<RID, NodeMetadata>,
    // Maps node complete, if all its edges are completed for particular inventory configuration
    // keys is computed inventory hash
    completed_nodes: HashMap<String, HashSet<RID>>,
    last_visited_node: ORID,
    response_buffer: String,
    first: ORID,
    head: ORID,
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
    output_is_available: bool,
    checkpoint_nodes: Vec<RID>,
}

#[derive(Debug)]
struct NodeMetadata {
    min_steps: u16,
    visits: u16,
    origin: ORID,
    from: OWID,
    edges_to_visit: Vec<String>,
    visited_edges: HashMap<String, u16>, // Stores number of visits
    last_visited_edge: Option<String>,
    // Marks edge completed for some set of inventory
    // In other words, with this tools this edge was completed if the node leading to this edge is completed too
    // Keys in inventory computed hash
    completed_edges: HashMap<String, String>,
    response_2_edge: HashMap<RID, String>,
    edge_2_response: HashMap<String, RID>,
    id: u16,
    auxiliary_commands: HashMap<String, String>,
    // IDEA: probably add here last response, or even response history to store the visited responses
    last_response: ResponseParts,
}

/// This struct is designed to immutable representation of the graph node identity
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct ResponseId {
    title: String,
    message: String,
    exits: Vec<String>,
}

impl From<ResponseParts> for ResponseId {
    fn from(parts: ResponseParts) -> Self {
        Self {
            title: parts.title,
            message: parts.message,
            exits: parts.exits,
        }
    }
}

impl From<&ResponseParts> for ResponseId {
    fn from(parts: &ResponseParts) -> Self {
        // Lets avoid unconditional recursion here
        // Self::from(parts.clone().deref())
        Self {
            title: parts.title.clone(),
            message: parts.message.clone(),
            exits: parts.exits.clone(),
        }
    }
}

impl From<ResponseParts> for RID {
    fn from(value: ResponseParts) -> Self {
        Rc::new(value.into())
    }
}

impl From<&ResponseParts> for RID {
    fn from(value: &ResponseParts) -> Self {
        Rc::new(value.into())
    }
}

impl fmt::Display for ResponseId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]: {}", self.title, self.message)
    }
}

impl ResponseId {
    fn new(title: String, message: String, exits: Vec<String>) -> Self {
        Self {
            title,
            message,
            exits,
        }
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
            checkpoint_nodes: Vec::new(),
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
        if output.is_empty() {
            return None;
        }
        let node_id = self.head.clone()?;
        self.nodes.entry(node_id).and_modify(|m| {
            let i = m.auxiliary_commands.insert(
                command.unwrap_or(CommandType::Empty).to_string(),
                output.clone(),
            );
            debug!("add {:?} to inventory of node {}", i, m.id)
        });
        Some(())
    }

    fn save_checkpoint(&mut self, node: RID) {
        debug!("save checkpoint {}", node);
        self.checkpoint_nodes.push(node.clone());
        match self.get_node_meta(&node) {
            Some(m) => {
                let commands = self
                    .get_full_path_back()
                    .into_iter()
                    .rev()
                    .map(|(_, _, cmd)| cmd)
                    .map(|cmd| cmd.unwrap_or("START".cyan().to_string()))
                    .collect::<Vec<String>>()
                    .join(" -> ");
                eprintln!("Commands: to node {}: {}", m.id, commands.yellow());
            }
            None => {
                warn!(
                    "checkpoint at initial node, though at least one starting position should be returned. Probably this is a bug. "
                );
            }
        }
    }
    fn add_response(&mut self, command: Option<CommandType>) -> Result<(), Box<dyn Error>> {
        if self.response_buffer.is_empty() {
            return Ok(());
        }
        let oan: OutputParser = OutputParser::new(self.response_buffer.as_str());
        let resp_parts = oan.parse()?;
        if resp_parts.dont_understand {
            debug!("this response means that program did not understand the previous command");
            trace!("flushing the buffer");
            self.flush();
            return Ok(());
        }
        if self.head.is_none() {
            // initial response but command exists (replay case)
            self.add_initial_response(resp_parts)?;
        } else {
            match command.clone() {
                Some(CommandType::InventoryTake(item)) => {
                    debug!("taking {} to inventory", item);
                    self.save_checkpoint(self.head.clone().unwrap());
                    let head = self.head.clone().ok_or("no head")?;
                    let last_response = self
                        .get_node_meta(&head)
                        .ok_or("no last_response")?
                        .last_response
                        .clone();
                    self.visit_edge(&head, command.clone().unwrap().to_string().as_str());
                    let things = &last_response.things_of_interest;
                    things.iter().for_each(|i| self.add_to_inventory(i.clone()));
                    self.set_aux_commands(resp_parts.pretext, command);
                    self.inventory_needs_update = true;
                }
                Some(CommandType::InventoryDrop(item)) => {
                    debug!("droppoing {} from inventory", item);
                    let head = self.head.clone().ok_or("no head")?;
                    self.visit_edge(&head, command.clone().unwrap().to_string().as_str());
                    let result = self.drop_from_inventory(&item);
                    if let Some(r) = result {
                        debug!(
                            "droppoing {} from inventory. It was {} times used, and {} times looked at",
                            item, r.0, r.1
                        );
                    } else {
                        debug!("nothing to drop");
                    }
                    self.inventory_needs_update = true;
                    self.set_aux_commands(resp_parts.pretext, command);
                }
                Some(CommandType::InventoryUse(item)) => {
                    debug!("using {} from inventory", item);
                    let head = self.head.clone().ok_or("no head")?;
                    self.visit_edge(&head, command.clone().unwrap().to_string().as_str());
                    (*self.inventory_global.entry(item).or_insert((0, 0))).0 += 1;
                    self.inventory_needs_update = true;
                    self.set_aux_commands(resp_parts.pretext, command);
                }
                Some(CommandType::InventoryLook(item)) => {
                    debug!("using {} from inventory", item);
                    let head = self.head.clone().ok_or("no head")?;
                    self.visit_edge(&head, command.clone().unwrap().to_string().as_str());
                    (*self.inventory_global.entry(item).or_insert((0, 0))).1 += 1;
                    self.set_aux_commands(resp_parts.pretext, command);
                }
                Some(CommandType::Inventory) => {
                    debug!("updating inventory");
                    let head = self.head.clone().ok_or("no head")?;
                    self.visit_edge(&head, command.clone().unwrap().to_string().as_str());
                    self.inventory_needs_update = self.update_inventory(resp_parts.inventory)?;
                    self.set_aux_commands(resp_parts.pretext, command);
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
            }
        }
        let head = self.head.clone().ok_or("no head")?;
        self.flush();
        self.commands_counter += 1;
        Ok(())
    }
    fn add_initial_response(
        &mut self,
        response_parts: ResponseParts,
    ) -> Result<(), Box<dyn Error>> {
        self.add_move_response(response_parts, None)
    }
    fn add_move_response(
        &mut self,
        mut resp_parts: ResponseParts,
        command: Option<CommandType>,
    ) -> Result<(), Box<dyn Error>> {
        let is_start_of_graph = self.head.is_none();
        debug!("moving to next node");
        let head_rid = self.head.clone();
        let head_steps_fallback_value = 0;

        // calculate origin
        let head_steps = head_rid
            .as_ref()
            .map(|rid| self.get_node_meta(&rid))
            .flatten()
            .map(|hm| hm.min_steps)
            .unwrap_or(head_steps_fallback_value);
        // let head_origin = head_rid.as_ref().map(|rid| self.get_node_meta(&rid)).flatten().map(|hm|hm.origin.clone()).flatten();
        let new_rid: RID = self.visit_node(resp_parts)?;
        let mut nm = self
            .get_node_meta_mut(&new_rid)
            .expect("at this point new node metadata must be present");
        let origin = if nm.min_steps < head_steps + 1 {
            nm.origin.clone()
        } else {
            head_rid.clone()
        };
        // let nm =    n_meta.as_mut().expect("at this point new node metadata must be present");
        nm.min_steps = min(nm.min_steps, head_steps + 1);
        nm.origin = origin;
        nm.from = head_rid.as_ref().map(|hr| Rc::downgrade(&hr));
        if let Err(edge_err) = self.optional_visit_edge(&head_rid, command) {
            debug!("failed to visit edge: {}", edge_err);
        }
        if let Err(link_err) =
            self.link_nodes(new_rid.clone(), head_rid.as_ref().map(Rc::downgrade))
        {
            debug!("linking error: {:?}", link_err);
        }

        self.head = Some(new_rid);
        if is_start_of_graph {
            self.first = self.head.clone();
        }
        Ok(())
    }
    fn drop_from_inventory(&mut self, item: &str) -> Option<(u16, u16)> {
        self.inventory_global.remove(item)
    }
    fn add_to_inventory(&mut self, item: String) {
        self.inventory_global.insert(item, (0, 0));
    }
    fn update_inventory(&mut self, items: Vec<String>) -> Result<bool, Box<dyn Error>> {
        // Update global inventory
        // Remove items from global inventory, which are not present in the argument
        let mut global_inv = &mut self.inventory_global;
        // Lets not differentiate between 'lit lantern' and 'lantern'
        let allowed: HashSet<&String> = items.iter().collect();
        global_inv.retain(|i, _| allowed.contains(i));
        allowed.into_iter().for_each(|i| {
            global_inv.entry(i.clone()).or_insert((0, 0));
        });
        Ok(false)
    }
    pub fn get_full_path_back(&self) -> Vec<(u16, String, Option<String>)> {
        let mut path: Vec<(u16, String, Option<String>)> = vec![];
        let mut current = self.head.clone();
        let mut cmd: Option<String> = None;
        while let Some(node) = current {
            let prev = self.get_prev_node(&node);
            match prev {
                Some(prev) => {
                    let prev_meta = self
                        .nodes
                        .get(&prev)
                        .expect("previous meta is absent, however the previous node exists");
                    let causing_edge = prev_meta.response_2_edge.get(&node).cloned();
                    path.push((
                        self.get_node_meta(&node).map(|m| m.id).unwrap_or(u16::MAX),
                        node.message.clone(),
                        cmd.clone(),
                    ));
                    cmd = causing_edge;
                }
                None => {
                    path.push((
                        self.get_node_meta(&node).map(|m| m.id).unwrap_or(u16::MAX),
                        node.message.clone(),
                        cmd.clone(),
                    ));
                }
            }
            current = self.get_prev_node(&node).clone()
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
            &self.head.clone().ok_or::<String>("no head".into())?.clone(),
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
                    }
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
            Some(head) => maze.push_str(&format!("{}{}\n", indentation, head)),
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
    fn get_exits_from_response(r: &RID) -> Vec<String> {
        r.exits.iter().map(|ex| format!("go {}", ex)).collect()
    }

    fn get_commands_from_response(r: &ResponseParts) -> Vec<String> {
        [
            // Lets try without look and help
            // &["look".to_string(), String::from("help")],
            Self::get_things_of_interest_from_response(r).as_slice(),
            // &[ "inv".to_string()],
            Self::get_inventory_from_response(r).as_slice(),
            Self::get_exits_from_response(&r.into()).as_slice(),
        ]
        // Revers it so .pop() will take the first value
        .concat()
        .iter()
        .rev()
        .cloned()
        .collect()
    }

    /// Validate how many steps left
    fn validate_steps_left(&self, node: &RID) -> Result<(), String> {
        if self.steps_left == 0 {
            return Err("exhausted steps".into());
        }
        Ok(())
    }

    fn get_node_meta(&self, node: &RID) -> Option<&NodeMetadata> {
        self.nodes.get(node)
    }
    fn get_prev_node_meta(&self, node: &RID) -> Option<&NodeMetadata> {
        let n_meta = self
            .get_node_meta(node)?
            .origin
            .iter()
            .map(|p| self.get_node_meta(p))
            .next()
            .flatten();
        n_meta
    }

    fn get_prev_node(&self, node: &RID) -> ORID {
        let prev = self.get_node_meta(node)?.origin.clone();
        prev
    }

    fn get_prev_node_resp_map(&self, node: &RID) -> Option<&HashMap<RID, String>> {
        let n_meta = self.get_prev_node_meta(node);
        let mapping = &n_meta?.response_2_edge;
        Some(mapping)
    }
    fn get_node_meta_mut(&mut self, node: &RID) -> Option<&mut NodeMetadata> {
        self.nodes.get_mut(node)
    }
    fn get_prev_node_meta_mut(&mut self, node: &RID) -> Option<&mut NodeMetadata> {
        let prev = self
            .get_node_meta(node)?
            .origin
            .clone()
            .map(|p| self.get_node_meta_mut(&p))
            .flatten();
        // self.get_node_meta_mut(n_meta?)
        prev
    }

    fn get_prev_node_resp_map_mut(&mut self, node: &RID) -> Option<&HashMap<RID, String>> {
        let n_meta = self.get_prev_node_meta_mut(node);
        let mapping = &mut n_meta?.response_2_edge;
        Some(mapping)
    }
    fn validate_go_back_command(node: &RID, cmd: &String) -> bool {
        Self::get_exits_from_response(node).contains(cmd)
    }
    fn get_command_back_to_previous(&self, node: &RID) -> Option<String> {
        let prev_mapping = self.get_prev_node_resp_map(node)?;
        let cause_command = prev_mapping.get(node)?.to_string();
        let oposite_command = match cause_command.as_str() {
            "go north" => "go south".to_string(),
            "go south" => "go north".to_string(),
            "go west" => "go east".to_string(),
            "go east" => "go west".to_string(),
            cmd => cmd.to_string(),
        };
        if Self::validate_go_back_command(node, &oposite_command) {
            Some(oposite_command)
        } else if Self::validate_go_back_command(node, &"go back".to_string()) {
            Some("go back".to_string())
        } else if node
            .message
            .contains("a twisty maze of little passages, all alike")
        {
            // If we are lost and cannot go back just pick a random one
            let directions = Self::get_exits_from_response(node);
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
    fn enqueue_commands(&mut self, node: &RID, visits_limit: u16) -> Result<(), String> {
        // Maze analyzer should compare the steps value of the previous node with the minimal value from the hash map.
        // If it is greater, than it means that it was not an optimal way to go. And commands should not be enqueued the second time.

        if self.inventory_needs_update {
            self.commands_queue.push_front("inv".to_string());
            Ok(())
        } else if let Some(cmd) = self.get_next_edge(node, visits_limit) {
            self.commands_queue.push_front(cmd);
            Ok(())
        } else {
            // Try to return to previous
            match self.get_command_back_to_previous(node) {
                Some(cmd) => Ok(self.commands_queue.push_front(cmd)),
                None => Err("No commands to visit, and cannot return back".into()),
            }
        }
    }
    // returns times node visited and min steps to visit it
    fn times_was_visited(&self, node: RID) -> (u16, u16) {
        if let Some(n_meta) = self.nodes.get(&node) {
            return (n_meta.visits, n_meta.min_steps);
        } else {
            (0, u16::MAX)
        }
    }

    fn link_nodes(&mut self, node: RID, from: OWID) -> Result<(), String> {
        if from.is_none() {
            debug!("nothing to link. Previous node is None.");
            return Ok(());
        }
        let fid = from.clone().unwrap();
        if fid.strong_count() == 0 {
            debug!("reference might be not valid anymore. Refusing to link");
            return Err("from argument does not have strong references".into());
        }
        let f_rid = fid.upgrade().ok_or("failed to upgrade weak ref")?;
        let f_meta = self
            .get_node_meta_mut(&f_rid)
            .ok_or("previous node metadata was not found")?;
        let original_edge = f_meta.last_visited_edge.clone().ok_or("no visited edges")?;
        f_meta
            .response_2_edge
            .insert(node.clone(), original_edge.clone());
        f_meta.edge_2_response.insert(original_edge.clone(), node);
        Ok(())
    }
    /// Creates node metadata or increments visits counter
    fn visit_node(&mut self, response: ResponseParts) -> Result<RID, String> {
        let rid = (&response).into();
        let id = self
            .get_node_meta(&rid)
            .map(|nm| nm.id)
            .unwrap_or(self.get_node_meta_id());
        self.nodes
            .entry(rid.clone())
            .or_insert(NodeMetadata {
                id,
                min_steps: u16::MAX,
                origin: None,
                from: None,
                edges_to_visit: Self::get_commands_from_response(&response),
                visits: 0,
                visited_edges: HashMap::new(),
                last_visited_edge: None,
                completed_edges: HashMap::new(),
                response_2_edge: HashMap::new(),
                edge_2_response: HashMap::new(),
                last_response: response,
                auxiliary_commands: HashMap::new(),
            })
            .visits += 1;
        self.last_visited_node = Some(rid.clone());
        Ok(rid)
    }

    fn get_node_meta_id(&mut self) -> u16 {
        let last = self.last_node_id.get_or_insert(self.nodes.len() as u16);
        *last += 1;
        *last
    }

    fn is_a_dangerous_edge(
        &self,
        node: &RID,
        command: &String,
        prev_command: Option<String>,
    ) -> bool {
        // No fear with lit lantern
        if self.inventory_global.get("lit lantern").is_some() {
            return false;
        }
        // Check for grues
        if node.message.contains("likely to be eaten by a") {
            return Self::analyse_dangerous_direction(&node.message, &command)
                .is_ok_and(|danger| danger);
            //return true;
        }
        if node
            .message
            .contains("become hopelessly lost and are fumbling around")
            && command.contains("forward")
        {
            return true;
        }
        if node.message.contains("you think you hear a Grue") {
            return Self::analyse_dangerous_direction(&node.message, &command)
                .is_ok_and(|danger| danger);
        }

        false
    }

    fn analyse_dangerous_direction(msg: &str, command: &str) -> Result<bool, Box<dyn Error>> {
        if command.contains("continue") {
            return Ok(true);
        }
        const DANGEROUS_PATTERNS: [&str; 2] = [
            r"The (?P<direction>.*) passage appears very dark",
            r"The passage to the (?P<direction>.*) looks very dark",
        ];
        let dangerous_regex = DANGEROUS_PATTERNS
            .iter()
            .map(|p| Regex::new(p).unwrap())
            .collect::<Vec<_>>();
        Ok(dangerous_regex
            .iter()
            .map(|re| re.captures(msg))
            .flatten()
            .map(|capt| capt.name("direction"))
            .flatten()
            .any(|dangerous| command.contains(dangerous.as_str())))
    }

    pub fn export_dot_graph(&self) -> Result<String, String> {
        let mut graph = dot_graph::DotGraph::new();
        let mut mapping: HashMap<RID, DotGraphNode> = HashMap::new();
        self.nodes.iter().for_each(|(node, meta)| {
            let notes = &meta.auxiliary_commands;
            let inv = self
                .inventory_global
                .keys()
                .map(String::as_str)
                .collect::<Vec<&str>>();
            let mut gn: DotGraphNode = dot_graph::DotGraphNode::new(
                meta.id,
                node.title.clone(),
                node.message.clone(),
                meta.min_steps,
                inv.as_slice(),
                notes,
            );
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
    fn get_next_edge(&mut self, node: &RID, max_times_visited: u16) -> Option<String> {
        let global_inv = self.inventory_global.clone();
        let to_prev_node = self.get_command_back_to_previous(node);
        let prev_command = self
            .get_prev_node_meta(node)
            .map(|pm| pm.last_visited_edge.clone())
            .flatten();
        let n_meta = self.get_node_meta(node).clone()?;
        let mut edges_to_visit: Vec<String> = n_meta
            .edges_to_visit
            .iter()
            .filter(|e| !n_meta.visited_edges.contains_key(*e))
            .filter(|e| !self.is_a_dangerous_edge(node, e, prev_command.clone()))
            .filter(|e| !Self::is_looked_or_used_inventory(&global_inv, e))
            .map(String::from)
            .collect::<Vec<String>>();
        while let Some(edge) = edges_to_visit.pop() {
            if edge == "use lit lantern" {
                // Just set it with low priority, to prevent using it
                // This prevents flipping 'lantern' to 'lit lantern' and using them in loop
                // self.nodes.entry(node.clone()).and_modify(|pm| {pm.visited_edges.insert(edge.clone(), u16::MAX/2); });
                self.get_node_meta_mut(node)
                    .map(|n| n.visited_edges.insert(edge, u16::MAX / 2));
                continue;
            }
            if to_prev_node.as_ref().is_some_and(|p| p.eq(&edge)) {
                // We do not want return back, unless there is no other choice. Decreasing priority.
                self.get_node_meta_mut(node)
                    .map(|pm| pm.visited_edges.insert(edge, 2));
                continue;
            }
            return Some(edge.clone());
        }
        self.get_next_edge_least_visited_fallback(node, max_times_visited)
    }

    fn get_next_edge_least_visited_fallback(
        &mut self,
        node: &RID,
        max_times_visited: u16,
    ) -> Option<String> {
        trace!("all edges have been consumed. Checking second round to find the least consumed");
        let n_meta = self.nodes.get(node)?;
        // And not visited for particular inventory version
        let last_visited_edge = n_meta.last_visited_edge.clone()?;
        let least_visited: Option<String> = n_meta
            .visited_edges
            .iter()
            .filter(|(k, v)| (**v) < max_times_visited)
            .filter(|(k, _)| matches!(CommandType::command_type(k), CommandType::Move(_)))
            .filter(|(k, _)| !k.as_str().eq(&last_visited_edge))
            .filter(|(k, _)| !self.is_a_dangerous_edge(node, k, Some(last_visited_edge.clone())))
            .filter(|(k, _)| self.get_completed_node_by_edge(k, node).is_none())
            .min_by(|(_key_1, val_1), (_key_2, val_2)| (**val_1).cmp(*val_2))
            .map(|(k, _v)| k.clone());
        least_visited
    }

    fn get_completed_node_by_edge(&self, edge: &str, node: &RID) -> ORID {
        let n_meta = self.nodes.get(node)?;
        let next_resp = n_meta.edge_2_response.get(edge).cloned()?;
        let inv_hash = self.global_inventory_hash();
        let completed_node = self
            .completed_nodes
            .get(&inv_hash)?
            .get(&next_resp)
            .cloned();
        completed_node
    }

    fn optional_visit_edge(
        &mut self,
        node: &ORID,
        command: Option<CommandType>,
    ) -> Result<(), String> {
        let n = node
            .clone()
            .ok_or("no node was provided for this command")?;
        let cmd: String = command
            .map(|c| c.to_string())
            .ok_or("command is none. Cannot visit edge.".to_string())?;
        self.visit_edge(&n, &cmd);
        Ok(())
    }
    fn visit_edge(&mut self, node: &RID, command: &str) {
        if let Some(n_meta) = self.get_node_meta_mut(node) {
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
                self.complete_node(node);
            }
        }
    }

    fn previous_is_accessible(&self, node: &RID) -> bool {
        node.exits.len() > 1 && self.get_command_back_to_previous(node).is_some()
    }

    fn complete_node(&mut self, node: &RID) {
        let inv_hash = self.global_inventory_hash();
        let prev = self.get_prev_node(node);
        let prev_completed = prev.is_none()
            || self
                .completed_nodes
                .get(&inv_hash)
                .map(|h| h.contains(&prev.unwrap()))
                .is_some_and(|b| b);
        if prev_completed {
            self.completed_nodes
                .entry(inv_hash)
                .or_insert(HashSet::new())
                .insert(node.clone());
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
        self.validate_steps_left(&node)?;
        const VISITS_LIMIT_PER_EDGE: u16 = 25;
        self.enqueue_commands(&node, VISITS_LIMIT_PER_EDGE)?;
        // We pop exactly 1 command, because new node will give other commands
        if let Some(cmd) = self.commands_queue.pop_front() {
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
        // This enables rambling / searching path
        self.steps_left += steps_limit;
        //  self.commands_counter += 1; //To expect output
    }
}
