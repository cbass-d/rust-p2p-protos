use color_eyre::eyre::Result;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Color,
    symbols::Marker,
    widgets::{
        Block, Borders, ListState, ScrollbarState, Widget,
        canvas::{Canvas, Circle, Line},
    },
};
use tracing::{debug, trace};

use crate::tui::{app::Action, components::node_list::Lists};

/// Radius of the circles representing the nodes
const RADIUS: f64 = 12.0;

#[derive(Debug, Clone)]
pub(crate) struct NodeCoords {
    x: f64,
    y: f64,
}

/// A display for the currently active nodes
/// Consists of a list that can be iterated through by
/// the user
#[derive(Debug, Clone)]
pub(crate) struct NodeBox {
    /// Hashset containig the list of active nodes, shared by the App
    /// and other components
    active_nodes: Arc<RwLock<IndexSet<PeerId>>>,

    /// Hashset containig the list of external nodes, shared by the App
    /// and other components
    external_nodes: Arc<RwLock<IndexSet<PeerId>>>,

    /// Which list has focus
    node_list: Lists,

    /// The state of the list of internal nodes (currently selected, next, etc.)
    internal_list_state: ListState,

    /// The state of the external list of nodes (currently selected, next, etc.) pub external_list_state: ListState,
    external_list_state: ListState,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,

    /// Hashmap representing the connections between the nodes
    node_connections: HashMap<PeerId, Arc<RwLock<HashSet<PeerId>>>>,

    /// X bound for the canvas
    x_bound: f64,

    /// Y bound for the canvas
    y_bound: f64,

    /// Cordinataes for each of the nodes
    node_coords: HashMap<PeerId, NodeCoords>,

    /// ratatui Circles for each of the nodes
    node_shapes: HashMap<PeerId, Circle>,

    /// Cordinataes for each of the external nodes
    external_node_coords: HashMap<PeerId, NodeCoords>,

    /// ratatui Circles for each of the external nodes
    external_node_shapes: HashMap<PeerId, Circle>,

    /// The lines connecting each of the nodes/circles
    lines: HashMap<(PeerId, PeerId), Line>,
}

impl NodeBox {
    pub fn new(
        active_nodes: Arc<RwLock<IndexSet<PeerId>>>,
        external_nodes: Arc<RwLock<IndexSet<PeerId>>>,
    ) -> Self {
        Self {
            active_nodes,
            external_nodes,
            list_state: ListState::default(),
            len: 0,
            x_bound: 180.0,
            y_bound: 90.0,
            focus: false,
            node_coords: HashMap::default(),
            node_shapes: HashMap::default(),
            external_node_coords: HashMap::default(),
            external_node_shapes: HashMap::default(),
            node_connections: HashMap::default(),
            internal_list_state: ListState::default(),
            external_list_state: ListState::default(),
            node_list: Lists::Internal,
            lines: HashMap::new(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = if self.focus {
            Block::new()
                .title("Node Graph")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new()
                .title("Node Graph")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
        };

        let nodes = &self.node_shapes;
        let external_nodes = &self.external_node_shapes;
        let connections = &self.lines;

        trace!(target: "app::node_box", "{} total nodes being drawn", nodes.len());

        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .block(block)
            .x_bounds([-self.x_bound, self.x_bound])
            .y_bounds([-self.y_bound, self.y_bound])
            .paint(|ctx| {
                // Draw connections first (behind)
                connections.iter().for_each(|(_, line)| ctx.draw(line));

                ctx.layer();

                // Draw nodes on top
                nodes.iter().for_each(|n| ctx.draw(n.1));

                // Draw external nodes on top
                external_nodes.iter().for_each(|n| ctx.draw(n.1));
            });

        canvas.render(area, frame.buffer_mut());
    }

    fn update_selection_in_graph(&mut self) {
        let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
        let active_nodes = self.active_nodes.read().clone();
        let external_nodes = self.external_nodes.read().clone();
        let peer = active_nodes[node_idx];

        // Update nodes
        self.reset_nodes();
        if let Some(circle) = self.node_shapes.get_mut(&peer) {
            circle.color = Color::Red;
        }

        // Update the lines
        self.reset_lines();
        self.lines
            .iter_mut()
            .filter(|(p, _)| p.0 == peer || p.1 == peer)
            .for_each(|(_, line)| {
                line.color = Color::Red;
            });
    }

    fn reset_nodes(&mut self) {
        self.node_shapes.iter_mut().for_each(|(_, circle)| {
            circle.color = Color::White;
        });

        self.external_node_shapes
            .iter_mut()
            .for_each(|(_, circle)| {
                circle.color = Color::Blue;
            });
    }

    fn reset_lines(&mut self) {
        self.lines.iter_mut().for_each(|(_, line)| {
            line.color = Color::White;
        });
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    fn generate_node_on_canvas(&mut self, peer: &PeerId, is_external: bool) -> Circle {
        let min_distance = 50.0;
        loop {
            let x = rand::random_range(-self.x_bound + 15.0..=self.x_bound - 15.0);
            let y = rand::random_range(-self.y_bound + 15.0..=self.y_bound - 15.0);

            let valid = self.node_coords.values().all(|coords| {
                let dx = x - coords.x;
                let dy = y - coords.y;
                (dx * dx + dy * dy).sqrt() >= min_distance
            });

            if valid {
                if is_external {
                    self.external_node_coords
                        .insert(peer.to_owned(), NodeCoords { x, y });
                    return Circle {
                        x,
                        y,
                        radius: RADIUS,
                        color: Color::Blue,
                    };
                } else {
                    self.node_coords
                        .insert(peer.to_owned(), NodeCoords { x, y });
                    return Circle {
                        x,
                        y,
                        radius: RADIUS,
                        color: Color::White,
                    };
                }
            }
        }
    }

    fn remove_connection(&mut self, peer_one: PeerId, peer_two: PeerId) {
        self.lines.remove(&(peer_one, peer_two));
        self.lines.remove(&(peer_two, peer_one));
    }

    fn update_connections(&mut self, peer_one: PeerId, peer_two: PeerId) {
        if !self.lines.contains_key(&(peer_one, peer_two)) {
            self.connect_two_nodes(peer_one, peer_two);
        }
    }

    fn connect_two_nodes(&mut self, peer_one: PeerId, peer_two: PeerId) {
        let circle_one = self.node_shapes.get(&peer_one);
        let circle_two = self.node_shapes.get(&peer_two);

        debug!(target: "app::node_box", "attempting to connect {peer_one} and {peer_two}");

        if let Some(circle_one) = circle_one
            && let Some(circle_two) = circle_two
        {
            debug!(target: "app::node_box", "two nodes being connected on graph {peer_one} {peer_two}");

            // Draw the line endpoints on the border of the circle
            // rather than the center
            let dx = circle_two.x - circle_one.x;
            let dy = circle_two.y - circle_one.y;
            let distance = (dx * dx + dy * dy).sqrt();

            let nx = dx / distance;
            let ny = dy / distance;

            let x1 = circle_one.x + nx * RADIUS;
            let y1 = circle_one.y + ny * RADIUS;

            let x2 = circle_two.x - nx * RADIUS;
            let y2 = circle_two.y - ny * RADIUS;

            let line = Line {
                x1,
                y1,
                x2,
                y2,
                color: Color::White,
            };

            self.lines.insert((peer_one, peer_two), line);
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        _actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                match self.node_list {
                    Lists::Internal => {
                        self.select_previous();
                        // Get the index of the newly selected node
                        self.update_selection_in_graph();
                    }
                    Lists::External => {}
                }
            }
            KeyCode::Down => {
                match self.node_list {
                    Lists::Internal => {
                        self.select_next();
                        // Get the index of the newly selected node
                        self.update_selection_in_graph();
                    }
                    Lists::External => {}
                }
            }
            KeyCode::Char('e') => {
                if !self.external_nodes.read().is_empty() {
                    self.node_list = match self.node_list {
                        Lists::Internal => Lists::External,
                        Lists::External => Lists::Internal,
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn select_next(&mut self) {
        self.list_state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.list_state.select_previous();
    }

    /// Moving up and down the listcan move past the bounds of the list,
    /// we must make sure it does not
    fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len {
            self.list_state.select(Some(self.len - 1));
            self.len - 1
        } else {
            idx
        }
    }

    fn remove_line(&mut self, peer_id: PeerId) {
        self.lines.retain(|p, _| p.0 != peer_id && p.1 != peer_id);
    }

    pub fn update(&mut self, action: &Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::AddNode {
                peer_id,
                node_connections,
            } => {
                debug!(target: "app::node_box", "adding new node {} with connections {:?}", peer_id, node_connections);

                self.len += 1;
                let node = self.generate_node_on_canvas(peer_id, false);
                self.node_shapes.insert(*peer_id, node);
                self.node_connections
                    .insert(*peer_id, node_connections.clone());

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));
                    self.update_selection_in_graph();

                    let active_nodes = self.active_nodes.read();
                    actions.push_back(Action::DisplayLogs {
                        peer_id: active_nodes[0],
                    });
                }
            }
            Action::AddExternalNode { peer_id } => {
                debug!(target: "app::node_box", "adding new external node {}", peer_id);

                let node = self.generate_node_on_canvas(peer_id, true);
                self.external_node_shapes.insert(*peer_id, node);
            }
            Action::RemoveNode { peer_id } => {
                self.len -= 1;

                self.node_coords.remove(peer_id);
                self.node_shapes.remove(peer_id);
                self.remove_line(*peer_id);
            }
            Action::UpdateConnections { peer_one, peer_two } => {
                self.update_connections(*peer_one, *peer_two);
            }
            Action::DisconnectFrom { peer_one, peer_two } => {
                self.remove_connection(*peer_one, *peer_two);
            }
            _ => {}
        }
    }
}
