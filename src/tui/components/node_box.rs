use color_eyre::eyre::Result;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::{PeerId, kad::Mode};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Color,
    symbols::Marker,
    widgets::{
        Block, Borders, ListState, Widget,
        canvas::{Canvas, Circle, Line, Painter, Rectangle, Shape},
    },
};
use tracing::{debug, trace};

use tracing::warn;

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    components::node_list::Lists,
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

/// Radius of the circles representing the client nodes
const RADIUS: f64 = 12.0;

// Width of the rectangle reprsenting the server nodes
const WIDTH: f64 = 18.0;

// Height of the rectangle reprsenting the server nodes
const HEIGHT: f64 = 18.0;

/// Enum wrapper for node shape drawn on the canvas
#[derive(Debug, Clone)]
enum NodeShape {
    Client(Circle),
    Server(Rectangle),
}

impl NodeShape {
    fn set_color(&mut self, color: Color) {
        match self {
            NodeShape::Client(circle) => circle.color = color,
            NodeShape::Server(rectangle) => rectangle.color = color,
        }
    }

    fn center(&self) -> (f64, f64) {
        match self {
            NodeShape::Client(c) => (c.x, c.y),
            NodeShape::Server(r) => (r.x + r.width / 2.0, r.y + r.height / 2.0),
        }
    }

    /// Returns the point on the shape's border along the ray from its center
    /// toward `target`. Used to draw connection lines that end on the edge
    /// of the shape rather than the center.
    fn border_point_toward(&self, target: (f64, f64)) -> (f64, f64) {
        let (cx, cy) = self.center();
        let dx = target.0 - cx;
        let dy = target.1 - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist == 0.0 {
            return (cx, cy);
        }
        let nx = dx / dist;
        let ny = dy / dist;

        match self {
            NodeShape::Client(c) => (cx + nx * c.radius, cy + ny * c.radius),
            NodeShape::Server(r) => {
                let hw = r.width / 2.0;
                let hh = r.height / 2.0;
                let tx = if nx.abs() > f64::EPSILON {
                    hw / nx.abs()
                } else {
                    f64::INFINITY
                };
                let ty = if ny.abs() > f64::EPSILON {
                    hh / ny.abs()
                } else {
                    f64::INFINITY
                };
                let t = tx.min(ty);
                (cx + nx * t, cy + ny * t)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NodeCoords {
    x: f64,
    y: f64,
}

/// Canvas showing active nodes as circles with connection lines.
#[derive(Debug, Clone)]
pub(crate) struct NodeBox {
    /// Which list has focus
    node_list: Lists,

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

    /// ratatui Circles for each of the client nodes
    nodes_shapes: HashMap<PeerId, NodeShape>,

    /// Cordinataes for each of the external nodes
    external_node_coords: HashMap<PeerId, NodeCoords>,

    /// ratatui Circles for each of the external nodes
    external_node_shapes: HashMap<PeerId, NodeShape>,

    /// The lines connecting each of the nodes/circles
    lines: HashMap<(PeerId, PeerId), Line>,
}

impl NodeBox {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            len: 0,
            x_bound: 180.0,
            y_bound: 90.0,
            focus: false,
            node_coords: HashMap::default(),
            nodes_shapes: HashMap::default(),
            external_node_shapes: HashMap::default(),
            external_node_coords: HashMap::default(),
            node_connections: HashMap::default(),
            node_list: Lists::Internal,
            lines: HashMap::new(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, _active_nodes: &IndexSet<PeerId>) {
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

        let nodes = &self.nodes_shapes;
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

    fn update_selection_in_graph(&mut self, active_nodes: &IndexSet<PeerId>) {
        // Update nodes
        self.reset_nodes();

        // Update the lines
        self.reset_lines();

        let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
        let peer = active_nodes[node_idx];

        if let Some(shape) = self.nodes_shapes.get_mut(&peer) {
            shape.set_color(Color::Red);
        }

        self.lines
            .iter_mut()
            .filter(|(p, _)| p.0 == peer || p.1 == peer)
            .for_each(|(_, line)| {
                line.color = Color::Red;
            });
    }

    fn reset_nodes(&mut self) {
        self.nodes_shapes.iter_mut().for_each(|(_, shape)| {
            shape.set_color(Color::White);
        });

        self.external_node_shapes.iter_mut().for_each(|(_, shape)| {
            shape.set_color(Color::Blue);
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

    fn generate_node_on_canvas(
        &mut self,
        peer: &PeerId,
        is_external: bool,
        mode: &Mode,
    ) -> NodeShape {
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
                match mode {
                    Mode::Client => {
                        if is_external {
                            self.external_node_coords
                                .insert(peer.to_owned(), NodeCoords { x, y });
                            return NodeShape::Client(Circle {
                                x,
                                y,
                                radius: RADIUS,
                                color: Color::Blue,
                            });
                        } else {
                            self.node_coords
                                .insert(peer.to_owned(), NodeCoords { x, y });
                            return NodeShape::Client(Circle {
                                x,
                                y,
                                radius: RADIUS,
                                color: Color::White,
                            });
                        }
                    }
                    Mode::Server => {
                        if is_external {
                            self.external_node_coords
                                .insert(peer.to_owned(), NodeCoords { x, y });
                            return NodeShape::Server(Rectangle {
                                x: x - WIDTH / 2.0,
                                y: y - HEIGHT / 2.0,
                                width: WIDTH,
                                height: HEIGHT,
                                color: Color::Blue,
                            });
                        } else {
                            self.node_coords
                                .insert(peer.to_owned(), NodeCoords { x, y });
                            return NodeShape::Server(Rectangle {
                                x: x - WIDTH / 2.0,
                                y: y - HEIGHT / 2.0,
                                width: WIDTH,
                                height: HEIGHT,
                                color: Color::White,
                            });
                        }
                    }
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
        let shape_one = self
            .nodes_shapes
            .get(&peer_one)
            .or_else(|| self.external_node_shapes.get(&peer_one));
        let shape_two = self
            .nodes_shapes
            .get(&peer_two)
            .or_else(|| self.external_node_shapes.get(&peer_two));

        debug!(target: "app::node_box", "attempting to connect {peer_one} and {peer_two}");

        if let Some(shape_one) = shape_one
            && let Some(shape_two) = shape_two
        {
            debug!(target: "app::node_box", "two nodes being connected on graph {peer_one} {peer_two}");

            let center_one = shape_one.center();
            let center_two = shape_two.center();

            let (x1, y1) = shape_one.border_point_toward(center_two);
            let (x2, y2) = shape_two.border_point_toward(center_one);

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
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                match self.node_list {
                    Lists::Internal => {
                        self.select_previous();
                        // Get the index of the newly selected node
                        self.update_selection_in_graph(active_nodes);
                    }
                    Lists::External => {}
                }
            }
            KeyCode::Down => {
                match self.node_list {
                    Lists::Internal => {
                        self.select_next();
                        // Get the index of the newly selected node
                        self.update_selection_in_graph(active_nodes);
                    }
                    Lists::External => {}
                }
            }
            KeyCode::Char('e') if !external_nodes.is_empty() => {
                self.node_list = match self.node_list {
                    Lists::Internal => Lists::External,
                    Lists::External => Lists::Internal,
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
            let idx = self.len.saturating_sub(1);
            self.list_state.select(Some(idx));
            idx
        } else {
            idx
        }
    }

    fn remove_line(&mut self, peer_id: PeerId) {
        self.lines.retain(|p, _| p.0 != peer_id && p.1 != peer_id);
    }

    pub fn update(
        &mut self,
        action: &Action,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
    ) {
        match action {
            Action::AddNode {
                peer_id,
                node_connections,
                mode,
            } => {
                debug!(target: "app::node_box", "adding new node {} with connections {:?}", peer_id, node_connections);

                self.len += 1;
                let node = self.generate_node_on_canvas(peer_id, false, mode);
                self.nodes_shapes.insert(*peer_id, node);
                self.node_connections
                    .insert(*peer_id, node_connections.clone());

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));
                    self.update_selection_in_graph(active_nodes);

                    actions.push(Action::DisplayLogs {
                        peer_id: active_nodes[0],
                    });
                }
            }
            Action::AddExternalNode { peer_id, .. } => {
                debug!(target: "app::node_box", "adding new external node {}", peer_id);

                let mode = &Mode::Server;
                let node = self.generate_node_on_canvas(peer_id, true, mode);
                self.external_node_shapes.insert(*peer_id, node);
            }
            Action::RemoveNode { peer_id } => {
                self.len -= 1;

                self.node_coords.remove(peer_id);
                self.nodes_shapes.remove(peer_id);
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

impl TuiEventHandler for NodeBox {
    fn on_key(
        &mut self,
        key: KeyEvent,
        _actions: &mut ActionQueue,
        ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(key, ctx.active_nodes, ctx.external_nodes) {
            warn!(target: "tui::node_box", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(&mut self, action: &Action, actions: &mut ActionQueue, active: &IndexSet<PeerId>) {
        self.update(action, actions, active);
    }
}

impl Shape for NodeShape {
    fn draw(&self, painter: &mut Painter) {
        match self {
            NodeShape::Client(circle) => circle.draw(painter),
            NodeShape::Server(rectangle) => rectangle.draw(painter),
        }
    }
}
