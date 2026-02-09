use color_eyre::eyre::Result;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock},
};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::{PeerId, core::connection};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    symbols::Marker,
    widgets::{
        Block, Borders, List, ListState, Widget,
        canvas::{Canvas, Circle, Line, Map, MapResolution, Rectangle},
    },
};
use tracing::{debug, trace};

use crate::tui::app::Action;

/// Radius of the circles representing the nodes
const RADIUS: f64 = 12.0;

#[derive(Debug, Clone)]
pub struct NodeCoords {
    x: f64,
    y: f64,
}

/// A display for the currently active nodes
/// Consists of a list that can be iterated through by
/// the user
#[derive(Debug, Clone)]
pub struct NodeBox {
    /// A IndexSet (a hashset that be accessed using []) of the actively
    /// running nodes that is used to build the list
    active_nodes: IndexSet<PeerId>,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,

    /// Hashmap representing the connections between the nodes
    node_connections: HashMap<PeerId, Arc<RwLock<HashSet<PeerId>>>>,

    x_bound: f64,
    y_bound: f64,

    node_coords: HashMap<PeerId, NodeCoords>,
    node_shapes: HashMap<PeerId, Circle>,
    lines: HashMap<(PeerId, PeerId), Line>,
}

impl NodeBox {
    pub fn new() -> Self {
        Self {
            active_nodes: IndexSet::new(),
            list_state: ListState::default(),
            len: 0,
            x_bound: 180.0,
            y_bound: 90.0,
            focus: false,
            node_coords: HashMap::default(),
            node_shapes: HashMap::default(),
            node_connections: HashMap::default(),
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
        let connections = &self.lines;

        trace!(target: "node_box", "{} total nodes being drawn", nodes.len());

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
            });

        canvas.render(area, frame.buffer_mut());
    }

    fn update_selection_in_graph(&mut self) {
        let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
        let peer = self.active_nodes[node_idx];
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
    }

    fn reset_lines(&mut self) {
        self.lines.iter_mut().for_each(|(_, line)| {
            line.color = Color::White;
        });
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    fn generate_node_on_canvas(&mut self, peer: &PeerId) -> Circle {
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

    //fn generate_connections(&mut self, peer_one: &PeerId, peer_two: &PeerID) {
    //    if let Some(connections) = self.node_connections.get(peer) {
    //        let connections = connections.read().unwrap().clone();
    //        for other_peer in connections {
    //            self.connect_two_nodes(peer, &other_peer);
    //        }
    //    }

    //    debug!(target: "node_box", "current lines: {:?}", self.lines);
    //}

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

        debug!(target: "node_box", "attempting to connect {peer_one} and {peer_two}");

        if circle_one.is_some() && circle_two.is_some() {
            debug!(target: "node_box", "two nodes being connected on graph {peer_one} {peer_two}");

            let circle_one = circle_one.unwrap();
            let circle_two = circle_two.unwrap();

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
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();

                // Get the index of the newly selected node
                self.update_selection_in_graph();
            }
            KeyCode::Down => {
                self.select_next();
                self.update_selection_in_graph();
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

    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::AddNode {
                peer_id,
                node_connections,
            } => {
                debug!(target: "node_box", "adding new node {} with connections {:?}", peer_id, node_connections);

                self.active_nodes.insert(peer_id);
                self.len += 1;

                let node = self.generate_node_on_canvas(&peer_id);
                self.node_shapes.insert(peer_id, node);
                self.node_connections.insert(peer_id, node_connections);

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));
                    self.update_selection_in_graph();

                    actions.push_back(Action::DisplayLogs {
                        peer_id: self.active_nodes[0],
                    });
                }
            }
            Action::RemoveNode { peer_id } => {
                self.active_nodes.swap_remove(&peer_id);
                self.len -= 1;

                self.node_coords.remove(&peer_id);
                self.node_shapes.remove(&peer_id);
                self.remove_line(peer_id);
            }
            Action::UpdateConnections { peer_one, peer_two } => {
                self.update_connections(peer_one, peer_two);
            }
            Action::DisconnectFrom { peer_one, peer_two } => {
                self.remove_connection(peer_one, peer_two);
            }
            _ => {}
        }
    }
}
