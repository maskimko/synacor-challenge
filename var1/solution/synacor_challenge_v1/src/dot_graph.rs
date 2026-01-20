use std::fmt;
use petgraph::data::Build;
use petgraph::dot::{Config, Dot};
use petgraph::graph::{DiGraph, NodeIndex};

#[derive(Debug, Clone)]
pub struct DotGraphNode {
    pub message: String,
    pub id: u16,
    pub label: String,
    inventory: Vec<String>,
    steps: u16,

    index: Option<NodeIndex>,
}

impl DotGraphNode {
    pub fn new(id: u16, title: String, message: String, steps: u16, inventory: &[&str]) -> DotGraphNode {
        DotGraphNode {
            id,
            message,
            steps,
            label: title,
            index: None,
            inventory: inventory.iter().map(|s| s.to_string()).collect(),
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

    fn dot_display(&self) -> String {
        let inventory: String =  if self.inventory.is_empty() {
            "<TD COLOR=\"#a0522d\"><I>Inventory is empty</I></TD>".to_string()
        } else {
            format!("<TD ALIGN=\"LEFT\"><B>Inventory:</B></TD>{}", self.inventory.iter().map(|s| format!("<TD ALIGN=\"CENTER\">{}</TD>", s)).collect::<String>())
        };
        format!(r#"shape="rect" style="rounded" label=<<TABLE BORDER="0" CELLBORDER="0" CELLSPACING="0">
                <TR><TD ALIGN="LEFT"><B>[{}]</B></TD><TD ALIGN="LEFT"><B>{}</B></TD><TD ALIGN="RIGHT"><I>Steps: {}</I></TD></TR>
                <HR/>
                <TR>{}</TR>
                <HR/>
                <TR><TD ALIGN="LEFT">{}</TD></TR>
            </TABLE>>"#, self.id, self.label, self.steps, inventory,self.message.replace('\n', "<BR/>"))
    }
}
impl fmt::Display for DotGraphNode {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
       write!(f, "[{}] {}\n{}", self.id, self.label, self.message)
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

    fn get_node_dot_attr(_graph: &DiGraph<DotGraphNode, String>, param: (NodeIndex, &DotGraphNode)) -> String {
         param.1.dot_display()
    }
    fn get_edge_dot_attr(_graph: &DiGraph<DotGraphNode, String>, param: petgraph::graph::EdgeReference<'_, String>) -> String {
        format!("label=\"{}\"", param.weight())
    }

    pub fn dot(&self) -> String {
        // let dot = Dot::new(&self.graph);
        let dot = Dot::with_attr_getters(&self.graph, &[Config::EdgeNoLabel, Config::NodeNoLabel], &Self::get_edge_dot_attr, &Self::get_node_dot_attr);
        format!("{:?}", dot)
    }
}
