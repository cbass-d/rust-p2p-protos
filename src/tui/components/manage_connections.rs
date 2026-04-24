use color_eyre::eyre::Result;
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListState, Padding, Paragraph, Widget},
};

use tracing::warn;

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    components::popup::PopUpContent,
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

/// Which list currently receives navigation and action keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Internal,
    External,
}

impl Focus {
    fn toggle(self) -> Self {
        match self {
            Focus::Internal => Focus::External,
            Focus::External => Focus::Internal,
        }
    }
}

/// Component for managing the connections for a node in the network. Connections can be
/// established or deleted.
#[derive(Debug)]
pub(crate) struct ManageConnections {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// Which of the two lists is currently focused
    focus: Focus,

    /// Selection state for the internal-peer list
    pub internal_state: ListState,

    /// Selection state for the external-peer list
    pub external_state: ListState,
}

impl ManageConnections {
    /// Build a fresh `ManageConnections` component
    pub fn new() -> Self {
        Self {
            node: None,
            focus: Focus::Internal,
            internal_state: ListState::default(),
            external_state: ListState::default(),
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    fn focused_state(&mut self) -> &mut ListState {
        match self.focus {
            Focus::Internal => &mut self.internal_state,
            Focus::External => &mut self.external_state,
        }
    }

    /// Active nodes excluding the current node — the peers you can connect to / disconnect from
    fn internal_peers(active_nodes: &IndexSet<PeerId>, current: PeerId) -> Vec<PeerId> {
        active_nodes
            .iter()
            .copied()
            .filter(|p| *p != current)
            .collect()
    }

    /// Resolve a selection against a list
    fn resolve_selection(state: &ListState, peers: &[PeerId]) -> Option<PeerId> {
        if peers.is_empty() {
            return None;
        }
        let idx = state.selected().unwrap_or(0).min(peers.len() - 1);
        Some(peers[idx])
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
        show_external: bool,
    ) -> Result<()> {
        let Some(peer_id) = self.node else {
            return Ok(());
        };

        // If external is hidden, force focus to internal regardless of prior state
        if !show_external && self.focus == Focus::External {
            self.focus = Focus::Internal;
        }

        match key_event.code {
            KeyCode::Tab if show_external => self.focus = self.focus.toggle(),
            KeyCode::Up => self.focused_state().select_previous(),
            KeyCode::Down => self.focused_state().select_next(),
            KeyCode::Char('c') => {
                if let Some(peer_two) = self.selected_target(active_nodes, external_nodes, peer_id)
                {
                    debug!(target: "app::manage_connections", %peer_two, "connecting to peer");
                    actions.push(Action::ConnectTo {
                        peer_one: peer_id,
                        peer_two,
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(peer_two) = self.selected_target(active_nodes, external_nodes, peer_id)
                {
                    debug!(target: "app::manage_connections", %peer_two, "disconnecting from peer");
                    actions.push(Action::DisconnectFrom {
                        peer_one: peer_id,
                        peer_two,
                    });
                }
            }
            KeyCode::Esc => {
                actions.push(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id,
                });
            }
            _ => {}
        }

        Ok(())
    }

    /// Resolve the currently-focused selection to a `PeerId`
    fn selected_target(
        &self,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
        current: PeerId,
    ) -> Option<PeerId> {
        match self.focus {
            Focus::Internal => {
                let internal = Self::internal_peers(active_nodes, current);
                Self::resolve_selection(&self.internal_state, &internal)
            }
            Focus::External => {
                let external: Vec<PeerId> = external_nodes.iter().copied().collect();
                Self::resolve_selection(&self.external_state, &external)
            }
        }
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
        node_connections: &HashMap<PeerId, HashSet<PeerId>>,
        show_external: bool,
    ) {
        Clear.render(area, frame.buffer_mut());

        if !show_external && self.focus == Focus::External {
            self.focus = Focus::Internal;
        }

        let mut footer_spans = vec![Span::raw("<Esc> exit"), Span::raw(", ")];
        if show_external {
            footer_spans.push(Span::raw("<Tab> switch list"));
            footer_spans.push(Span::raw(", "));
        }
        footer_spans.extend([
            Span::raw("<Up> <Down> select peer"),
            Span::raw(", "),
            Span::raw("<c> connect"),
            Span::raw(", "),
            Span::raw("<d> disconnect"),
        ]);
        let footer_text = Line::from(footer_spans).style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("Manage Connections")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let constraints: &[Constraint] = if show_external {
            &[
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ]
        } else {
            &[Constraint::Percentage(20), Constraint::Percentage(80)]
        };
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner_area);
        let title_area = areas[0];
        let internal_area = areas[1];
        let external_area = if show_external { Some(areas[2]) } else { None };

        let current_node_title = self
            .node
            .map(|peer| format!("Current Node: {peer}"))
            .unwrap_or_else(|| "No node selected".to_string());

        let title_paragraph = Paragraph::new(
            Text::from(Line::from(current_node_title)).style(Style::new().underlined()),
        );
        frame.render_widget(title_paragraph, title_area);

        let Some(node) = self.node else {
            return;
        };

        let connections = node_connections.get(&node);

        let internal = Self::internal_peers(active_nodes, node);
        trace!(target: "app::manage_connections", "internal peer list: {internal:#?}");

        self.render_list(
            frame,
            internal_area,
            "Internal Nodes",
            &internal,
            connections,
            Focus::Internal,
        );

        if let Some(external_area) = external_area {
            let external: Vec<PeerId> = external_nodes.iter().copied().collect();
            self.render_list(
                frame,
                external_area,
                "External Nodes",
                &external,
                connections,
                Focus::External,
            );
        }
    }

    fn render_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        peers: &[PeerId],
        connections: Option<&HashSet<PeerId>>,
        kind: Focus,
    ) {
        let focused = self.focus == kind;
        let border_color = if focused {
            Color::LightCyan
        } else {
            Color::DarkGray
        };
        let block = Block::new()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_color);

        if peers.is_empty() {
            let msg = match kind {
                Focus::Internal => "--- No internal peers ---",
                Focus::External => "--- No external peers ---",
            };
            Paragraph::new(msg)
                .alignment(Alignment::Center)
                .block(block)
                .render(area, frame.buffer_mut());
            return;
        }

        let state = match kind {
            Focus::Internal => &mut self.internal_state,
            Focus::External => &mut self.external_state,
        };

        let max_idx = peers.len() - 1;
        match state.selected() {
            Some(sel) if sel > max_idx => state.select(Some(max_idx)),
            None => state.select(Some(0)),
            _ => {}
        }

        let list_items = format_peer_list(peers.iter(), connections);
        let highlight_style = if focused {
            Style::new().reversed()
        } else {
            Style::new()
        };
        let list = List::new(list_items)
            .highlight_style(highlight_style)
            .block(block);
        frame.render_stateful_widget(list, area, state);
    }

    pub fn update(
        &mut self,
        action: &Action,
        _actions: &mut ActionQueue,
        _active_nodes: &IndexSet<PeerId>,
    ) {
        match action {
            Action::DisplayManageConnections { peer_id } => {
                self.node = Some(*peer_id);
                self.focus = Focus::Internal;
                if self.internal_state.selected().is_none() {
                    self.internal_state.select(Some(0));
                }
            }
            _ => {}
        }
    }
}

impl TuiEventHandler for ManageConnections {
    fn on_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        actions: &mut ActionQueue,
        ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(
            key,
            actions,
            ctx.active_nodes,
            ctx.external_nodes,
            ctx.transport_tcp,
        ) {
            warn!(target: "tui::manage_connections", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(&mut self, action: &Action, actions: &mut ActionQueue, active: &IndexSet<PeerId>) {
        self.update(action, actions, active);
    }
}

/// Format peer entries showing connection status relative to `connections`.
/// `[*]` = connected, `[ ]` = not connected (or unknown).
fn format_peer_list<'a>(
    peers: impl Iterator<Item = &'a PeerId>,
    connections: Option<&HashSet<PeerId>>,
) -> Vec<String> {
    peers
        .map(|p| match connections {
            Some(conns) if conns.contains(p) => format!("[*] {p}"),
            _ => format!("[ ] {p}"),
        })
        .collect()
}
