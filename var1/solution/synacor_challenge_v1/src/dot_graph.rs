use petgraph::data::Build;
use petgraph::dot::{Config, Dot};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub struct DotGraphNode {
    pub message: String,
    pub id: u16,
    pub label: String,
    inventory: Vec<String>,
    steps: u16,
    notes: HashMap<String, String>,
    index: Option<NodeIndex>,
}

impl DotGraphNode {
    pub fn new(
        id: u16,
        title: String,
        message: String,
        steps: u16,
        inventory: &[&str],
        notes: &HashMap<String, String>,
    ) -> DotGraphNode {
        DotGraphNode {
            id,
            message,
            steps,
            label: title,
            index: None,
            inventory: inventory.iter().map(|s| s.to_string()).collect(),
            notes: notes.clone(),
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
        const BG: &str = "#2D2E27"; // node panel
        const BG2: &str = "#31332B"; // subtle strip
        const BORDER: &str = "#75715E"; // monokai "comment"/border
        const TEXT: &str = "#F8F8F2"; // main text
        const MUTED: &str = "#CFCFC2"; // secondary text
        const YELLOW: &str = "#E6DB74"; // title
        const MAGENTA: &str = "#F92672"; // id
        const CYAN: &str = "#66D9EF"; // steps / links
        const GREEN: &str = "#A6E22E"; // inventory items
        const PURPLE: &str = "#AE81FF"; // command
        const ORANGE: &str = "#FD971F"; // optional accent

        // Inventory row always spans 8 columns total (1 + 7)
        let inventory: String = if self.inventory.is_empty() {
            format!(
                r#"<TD COLSPAN="1" WIDTH="95" ALIGN="RIGHT" BGCOLOR="{bg2}">
                  <B><FONT COLOR="{muted}">Inventory:</FONT></B>
               </TD>
               <TD COLSPAN="7" ALIGN="LEFT" BGCOLOR="{bg}">
                  <I><FONT COLOR="{border}">empty</FONT></I>
               </TD>"#,
                bg = BG,
                bg2 = BG2,
                muted = MUTED,
                border = BORDER
            )
        } else {
            let items = self
                .inventory
                .iter()
                .map(|s| {
                    format!(
                        r#"<TD BGCOLOR="{tag_bg}" ALIGN="CENTER">
                      <B><FONT COLOR="{green}">{}</FONT></B>
                   </TD>"#,
                        s,
                        tag_bg = "#3B3C35",
                        green = GREEN
                    )
                })
                .collect::<String>();

            format!(
                r#"<TD COLSPAN="1" WIDTH="95" ALIGN="RIGHT" BGCOLOR="{bg2}">
                  <B><FONT COLOR="{muted}">Inventory:</FONT></B>
               </TD>
               <TD COLSPAN="7" ALIGN="LEFT" BGCOLOR="{bg}">
                 <TABLE BORDER="0" CELLBORDER="1" COLOR="{border}" CELLSPACING="0" CELLPADDING="4">
                   <TR>{}</TR>
                 </TABLE>
               </TD>"#,
                items,
                bg = BG,
                bg2 = BG2,
                muted = MUTED,
                border = BORDER
            )
        };

        // Notes: each row spans 8 columns total (1 + 7)
        let notes: String = if self.notes.is_empty() {
            "".to_string()
        } else {
            let rows = self.notes.iter().map(|(o, c)| {
                format!(
                    r#"<TR>
                     <TD WIDTH="120" ALIGN="RIGHT" BGCOLOR="{bg2}" BORDER="1" COLOR="{border}" TITLE="Command">
                       <B><FONT COLOR="{purple}">{}</FONT></B>
                     </TD>
                     <TD COLSPAN="7" ALIGN="LEFT" BGCOLOR="{out_bg}" BORDER="1" COLOR="{border}" TITLE="Output">
                       <FONT COLOR="{cyan}">{}</FONT>
                     </TD>
                   </TR>"#,
                    o, c,
                    bg2=BG2,
                    out_bg="#1F201B",
                    border=BORDER,
                    purple=PURPLE,
                    cyan=CYAN
                )
            }).collect::<String>();

            format!(r#"<HR/>{}"#, rows)
        };

        let message = self.message.replace('\n', "<BR/>");

        format!(
            r###"shape="rect"
style="rounded,filled"
fillcolor="{bg}"
color="{border}"
penwidth="1.3"
fontname="Inter"
fontsize="10"
margin="0.04,0.03"
label=<<TABLE BORDER="0" CELLBORDER="0" CELLSPACING="0" CELLPADDING="6" BGCOLOR="{bg}">

  <TR>
    <TD WIDTH="62" FIXEDSIZE="TRUE" ALIGN="CENTER" BGCOLOR="{bg2}">
      <B><FONT COLOR="{magenta}">[{id}]</FONT></B>
    </TD>
    <TD COLSPAN="6" ALIGN="LEFT" BGCOLOR="{bg2}">
      <B><FONT COLOR="{yellow}">{title}</FONT></B>
    </TD>
    <TD WIDTH="110" FIXEDSIZE="TRUE" ALIGN="RIGHT" BGCOLOR="{bg2}">
      <I><FONT COLOR="{cyan}">Steps: {steps}</FONT></I>
    </TD>
  </TR>

  <TR>
    <TD COLSPAN="8" BGCOLOR="{border}" HEIGHT="1"></TD>
  </TR>

  <TR>{inventory}</TR>

  <TR>
    <TD COLSPAN="8" BGCOLOR="{border}" HEIGHT="1"></TD>
  </TR>

  <TR>
    <TD COLSPAN="8" ALIGN="LEFT" BGCOLOR="{bg}">
      <FONT COLOR="{text}">{message}</FONT>
    </TD>
  </TR>

  {notes}

</TABLE>>"###,
            bg = BG,
            bg2 = BG2,
            border = BORDER,
            text = TEXT,
            magenta = MAGENTA,
            yellow = YELLOW,
            cyan = CYAN,
            inventory = inventory,
            notes = notes,
            id = self.id,
            title = self.label,
            steps = self.steps,
            message = message
        )
    }
}
impl<'a> fmt::Display for DotGraphNode {
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

    fn get_node_dot_attr(
        _graph: &DiGraph<DotGraphNode, String>,
        param: (NodeIndex, &DotGraphNode),
    ) -> String {
        param.1.dot_display()
    }
    fn get_edge_dot_attr(
        _graph: &DiGraph<DotGraphNode, String>,
        param: petgraph::graph::EdgeReference<'_, String>,
    ) -> String {
        format!("label=\"{}\"", param.weight())
    }

    pub fn dot(&self) -> String {
        // let dot = Dot::new(&self.graph);
        let dot = Dot::with_attr_getters(
            &self.graph,
            &[Config::EdgeNoLabel, Config::NodeNoLabel],
            &Self::get_edge_dot_attr,
            &Self::get_node_dot_attr,
        );
        format!("{:?}", dot)
    }
}
