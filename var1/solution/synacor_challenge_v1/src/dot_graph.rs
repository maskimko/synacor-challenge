use std::fmt;
use petgraph::data::Build;
use petgraph::dot::Dot;
use petgraph::graph::{DiGraph, NodeIndex};

#[derive(Debug, Clone)]
pub struct DotGraphNode {
    pub message: String,
    pub id: u16,
    pub label: String,
    index: Option<NodeIndex>,
}

impl fmt::Display for DotGraphNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl DotGraphNode {
    pub fn new(id: u16, title: String, message: String) -> DotGraphNode {
        DotGraphNode {
            id,
            message,
            label: title,
            index: None,
        }
    }

    fn reindex(self, index: NodeIndex) -> DotGraphNode {
        Self {
            index: Some(index),
            ..self
        }
    }

    pub fn index(&self) -> Option<NodeIndex> {
        self.index.clone()
    }
}

#[derive(Debug)]
pub struct DotGraph {
    graph: DiGraph<DotGraphNode, String>,
}

impl DotGraph {
    pub fn new() -> DotGraph {
        DotGraph {
            graph: DiGraph::new(),
        }
    }

    pub fn add_node(&mut self, node: DotGraphNode) -> DotGraphNode {
        let index = self.graph.add_node(node.clone());
        node.reindex(index)
    }

    pub fn add_edge(&mut self, from: &DotGraphNode, to: &DotGraphNode, command: String) {
        self.graph
            .add_edge(from.index.unwrap(), to.index.unwrap(), command);
    }

    pub fn dot(&self) -> String {
        let dot = Dot::new(&self.graph);
        format!("{:?}", dot)
    }
}
