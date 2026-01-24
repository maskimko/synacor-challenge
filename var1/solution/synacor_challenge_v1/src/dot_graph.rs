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
    visits: u16,
    visited_edges_num: u16,
    edges_num:  u16,
    edges: HashMap<String, u16> //shows visited edges
}

impl DotGraphNode {
    pub fn new(
        id: u16,
        title: String,
        message: String,
        steps: u16,
        inventory: Vec<String>,
        notes: &HashMap<String, String>,
        visits: u16,
        visited_edges_num: u16,
        edges_num:  u16,
        edges: HashMap<String,u16>
    ) -> DotGraphNode {
        DotGraphNode {
            id,
            message,
            steps,
            label: title,
            index: None,
            inventory,
            notes: notes.clone(),
            visits,
            visited_edges_num,
            edges_num,
            edges,
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
        const BLUE_V_EDGE: &str = "#a358FF"; // for visited edges
        const BLUE_EDGE: &str = "#9378FF"; // for edges
        const RED_INCOMPLETE : &str = "#FC5345"; //To mark incomplete node

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


        let exits: String = if self.edges.is_empty() {
            "".to_string()
        } else {
            let rows = self.edges.iter().map(|(o, c)| {
                let visited_color = if *c > 0 {
                    TEXT
                } else {
                    RED_INCOMPLETE
                };
                format!(
                    r#"<TR>
                     <TD WIDTH="120" ALIGN="RIGHT" BGCOLOR="{bg2}" BORDER="1" COLOR="{border}" TITLE="Exit">
                       <B><FONT COLOR="{yellow}">{}</FONT></B>
                     </TD>
                     <TD COLSPAN="7" ALIGN="LEFT" BGCOLOR="{out_bg}" BORDER="1" COLOR="{border}" TITLE="Visited">
                       <FONT COLOR="{visited_color}">{}</FONT>
                     </TD>
                   </TR>"#,
                    o, c,
                    bg2=BG2,
                    out_bg="#1F201B",
                    border=BORDER,
                    yellow=YELLOW,
                    visited_color=visited_color,
                )
            }).collect::<String>();

            format!(r#"<HR/><TR><TD COLSPAN="8" ALIGN="CENTER">Exits</TD></TR>{}"#, rows)
        };

        let message = self.message.replace('\n', "<BR/>");

        let visited_title_color: &str = if self.visited_edges_num == self.edges_num {
            TEXT
        } else {
            RED_INCOMPLETE
        };
        format!(
            r###"shape="rect"
style="rounded,filled"
fillcolor="{bg}"
color="{border}"
penwidth="1.3"
fontname="Inter"
fontsize="25"
margin="0.04,0.03"
label=<<TABLE BORDER="0" CELLBORDER="0" CELLSPACING="0" CELLPADDING="6" BGCOLOR="{bg}">

  <TR>
    <TD WIDTH="62" ALIGN="CENTER" BGCOLOR="{bg2}">
      <B><FONT COLOR="{magenta}">[{id}]</FONT></B>
    </TD>
    <TD COLSPAN="6" ALIGN="LEFT" BGCOLOR="{bg2}">
      <B><FONT COLOR="{yellow}">{title}</FONT></B>
    </TD>
    <TD WIDTH="110" ALIGN="RIGHT" BGCOLOR="{bg2}">
      <I><FONT COLOR="{cyan}">Steps: {steps}</FONT></I>
    </TD>
    <TD WIDTH="110" ALIGN="RIGHT" BGCOLOR="{bg2}">
      <I><FONT COLOR="{orange}">Visits: {visits}</FONT></I>
    </TD>
    <TD WIDTH="110" ALIGN="RIGHT" BGCOLOR="{bg2}">
      <I><FONT COLOR="{visit_title}">Visited edges: </FONT></I>(<FONT COLOR="{blue_visited}">{visited_num}</FONT>/<FONT COLOR="{blue_edges}">{edges_num}</FONT>)
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
  {exits}

</TABLE>>"###,
            bg = BG,
            bg2 = BG2,
            border = BORDER,
            text = TEXT,
            magenta = MAGENTA,
            yellow = YELLOW,
            orange = ORANGE,
            cyan = CYAN,
            inventory = inventory,
            notes = notes,
            id = self.id,
            title = self.label,
            steps = self.steps,
            visits = self.visits,
            blue_visited = BLUE_V_EDGE,
            blue_edges = BLUE_EDGE,
            visited_num = self.visited_edges_num,
            edges_num = self.edges_num,
            visit_title = visited_title_color,
            message = message,
            exits = exits,
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
        let mut s = format!("{:?}", dot);
        // Inject right after the opening brace
        const HEADER: &str = r##"
  graph [
    bgcolor="#272822",
    rankdir=LR,
    splines=true,          // curved lines (instead of squared)
    nodesep=0.35,
    ranksep=0.9,
    pad=0.25,
    newrank=true,
    concentrate=false      // do not merge edges! To show backwards movements
  ];
  node  [
    fontname="Inter",
    fontsize=20,
    shape=rect,
    style="rounded,filled",
    fillcolor="#2D2E27",
    color="#75715E",
    fontcolor="#F8F8F2",
    penwidth=1.3
  ];
  edge  [
    color="#66D9EF",
    fontcolor="#F8F8F2",
    penwidth=1.1,
    arrowsize=0.75,
    fontname="Inter",
    fontsize=15,
    // Helps separate A->B from B->A
    // (a little "fan-out" for multiple edges between same nodes)
    minlen=1
  ];
"##;

        if let Some(pos) = s.find('{') {
            // insert after "{\n" (or just after "{")
            let insert_at = pos + 1;
            s.insert_str(insert_at, HEADER);
        }

        s
    }
}
