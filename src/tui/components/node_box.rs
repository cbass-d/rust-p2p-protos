use std::{
    collections::{HashMap, HashSet},
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
use tracing::debug;

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
}

impl NodeBox {
    pub fn new() -> Self {
        Self {
            active_nodes: IndexSet::new(),
            list_state: ListState::default(),
            len: 0,
            x_bound: 180.0,
            y_bound: 90.0,
            node_coords: HashMap::default(),
            node_shapes: HashMap::default(),
            node_connections: HashMap::default(),
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = if self.focus {
            Block::new()
                .title("Node Graph")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new().title("Node Graph").borders(Borders::ALL)
        };

        let nodes = &self.node_shapes;
        let connections = &self.node_connections;

        let connection_snapshot: HashMap<PeerId, HashSet<PeerId>> = connections
            .iter()
            .filter_map(|(id, lock)| lock.try_read().ok().map(|peers| (*id, peers.clone())))
            .collect();

        debug!(target: "node_box", "{} total nodes being drawn", nodes.len());

        let canvas = Canvas::default()
            .marker(Marker::Sextant)
            .block(block)
            .x_bounds([-self.x_bound, self.x_bound])
            .y_bounds([-self.y_bound, self.y_bound])
            .paint(|ctx| {
                // Draw connections first (behind)
                nodes.iter().for_each(|n| {
                    if let Some(peers) = connection_snapshot.get(n.0) {
                        for peer in peers.iter() {
                            if let Some(other) = nodes.get(peer) {
                                ctx.draw(&Line {
                                    x1: n.1.x,
                                    y1: n.1.y,
                                    x2: other.x,
                                    y2: other.y,
                                    color: Color::White,
                                });
                            }
                        }
                    }
                });

                ctx.layer();

                // Draw nodes on top
                nodes.iter().for_each(|n| ctx.draw(n.1));
            });

        canvas.render(area, frame.buffer_mut());
    }

    fn update_selection_in_graph(&mut self, peer: PeerId) {
        self.reset_nodes();
        if let Some(circle) = self.node_shapes.get_mut(&peer) {
            circle.color = Color::Red;
        }
    }

    fn reset_nodes(&mut self) {
        self.node_shapes.iter_mut().for_each(|(_, circle)| {
            circle.color = Color::White;
        });
    }

    fn generate_node_on_canvas(&mut self, peer: PeerId) -> Circle {
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
                self.node_coords.insert(peer, NodeCoords { x, y });

                return Circle {
                    x,
                    y,
                    radius: RADIUS,
                    color: Color::White,
                };
            }
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                self.update_selection_in_graph(self.active_nodes[node_idx]);
                Some(Action::DisplayLogs(self.active_nodes[node_idx]))
            }
            KeyCode::Down => {
                self.select_next();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                self.update_selection_in_graph(self.active_nodes[node_idx]);
                Some(Action::DisplayLogs(self.active_nodes[node_idx]))
            }
            _ => None,
        }
    }

    pub fn select_next(&mut self) {
        self.list_state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.list_state.select_previous();
    }

    /// Moving up and down the listcan move past the bounds of the list,
    /// we must make sure it does not
    pub fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len {
            self.len - 1
        } else if idx < 0 {
            0
        } else {
            idx
        }
    }

    pub fn update(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::AddNode {
                peer_id,
                node_connections,
            } => {
                debug!(target: "node_box", "adding new node {} with connections {:?}", peer_id, node_connections);

                self.active_nodes.insert(peer_id);
                self.len += 1;

                let node = self.generate_node_on_canvas(peer_id);
                self.node_shapes.insert(peer_id, node);
                self.node_connections.insert(peer_id, node_connections);

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));

                    Some(Action::DisplayLogs(self.active_nodes[0]))
                } else {
                    None
                }
            }
            Action::RemoveNode(peer) => {
                self.active_nodes.swap_remove(&peer);
                self.len -= 1;

                None
            }
            _ => None,
        }
    }
}
