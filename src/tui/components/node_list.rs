use color_eyre::eyre::Result;
use indexmap::IndexSet;
use std::collections::HashMap;
use tracing::debug;

use crossterm::event::{KeyCode, KeyEvent};
use libp2p::{PeerId, kad::Mode};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use tracing::warn;

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    components::popup::PopUpContent,
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

#[derive(Debug, Clone)]
pub(crate) enum Lists {
    Internal,
    External,
}

/// Scrollable list of the currently active nodes.
#[derive(Debug, Clone)]
pub(crate) struct NodeList {
    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list of internal nodes (currently selected, next, etc.)
    internal_list_state: ListState,

    /// The state of the external list of nodes (currently selected, next, etc.) pub external_list_state: ListState,
    external_list_state: ListState,

    internal_scrollbar_state: ScrollbarState,

    external_scrollbar_state: ScrollbarState,

    /// Which list has focus
    node_list: Lists,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl NodeList {
    pub fn new() -> Self {
        Self {
            internal_list_state: ListState::default(),
            external_list_state: ListState::default(),
            len: 0,
            internal_scrollbar_state: ScrollbarState::default(),
            external_scrollbar_state: ScrollbarState::default(),
            node_list: Lists::Internal,
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    fn clap_scrollbar_pos(
        &self,
        new_pos: usize,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
    ) -> usize {
        let len = match self.node_list {
            Lists::Internal => active_nodes.len(),
            Lists::External => external_nodes.len(),
        };
        if new_pos > len { len } else { new_pos }
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
    ) {
        let (internal_area, external_area) = if external_nodes.is_empty() {
            (area, None)
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            (chunks[0], Some(chunks[1]))
        };

        let mut block = if self.focus {
            match self.node_list {
                Lists::Internal => Block::new()
                    .title("Nodes")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(Color::LightRed),
                Lists::External => Block::new()
                    .title("Nodes")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(Color::White),
            }
        } else {
            Block::new().title("Nodes").borders(Borders::ALL)
        };

        // Hashmap of idx in the list to the peer id
        let mut idx_to_peer = HashMap::new();
        let mut nodes = vec![];
        active_nodes.iter().enumerate().for_each(|(i, p)| {
            idx_to_peer.insert(i, p);
            nodes.push(p);
        });

        if active_nodes.len() < 10 && !external_nodes.is_empty() {
            block = block
                .title_bottom("<s> Add server node <c> Add client node <e> Switch to external");
        } else if active_nodes.len() < 10 && external_nodes.is_empty() {
            block = block.title_bottom("<s> Add server node <c> Add client node");
        } else {
            block = block.title_bottom("MAX NODES");
        }

        let internal_list = List::new(
            nodes
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<String>>(),
        )
        .highlight_style(Style::new().reversed())
        .highlight_symbol(">")
        .block(block);

        frame.render_stateful_widget(internal_list, internal_area, &mut self.internal_list_state);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        self.internal_scrollbar_state = self
            .internal_scrollbar_state
            .content_length(active_nodes.len());
        self.internal_scrollbar_state = self
            .internal_scrollbar_state
            .viewport_content_length(internal_area.height as usize);

        frame.render_stateful_widget(
            scrollbar,
            internal_area.inner(Margin {
                vertical: 2,
                horizontal: 0,
            }),
            &mut self.internal_scrollbar_state,
        );

        if let Some(external_area) = external_area {
            let external_block = match self.node_list {
                Lists::Internal => Block::new()
                    .title("External Nodes")
                    .title_alignment(Alignment::Center)
                    .title_bottom("<e> Switch to internal")
                    .borders(Borders::ALL)
                    .border_style(Color::White),
                Lists::External => Block::new()
                    .title("External Nodes")
                    .title_bottom("<e> Switch to internal")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_style(Color::LightRed),
            };

            let external_list = List::new(
                external_nodes
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<String>>(),
            )
            .highlight_style(Style::new().reversed())
            .highlight_symbol(">")
            .block(external_block);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            self.external_scrollbar_state = self
                .external_scrollbar_state
                .content_length(external_nodes.len());
            self.external_scrollbar_state = self
                .external_scrollbar_state
                .viewport_content_length(external_area.height as usize);

            frame.render_stateful_widget(
                external_list,
                external_area,
                &mut self.external_list_state,
            );
            frame.render_stateful_widget(
                scrollbar,
                external_area.inner(Margin {
                    vertical: 2,
                    horizontal: 0,
                }),
                &mut self.external_scrollbar_state,
            );
        }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();

                // Get the index of the newly selected node
                let node_idx = match self.node_list {
                    Lists::Internal => self.clamp(self.internal_list_state.selected().unwrap_or(0)),
                    Lists::External => self.clamp(self.external_list_state.selected().unwrap_or(0)),
                };

                match self.node_list {
                    Lists::External => {
                        self.external_scrollbar_state =
                            self.external_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.external_scrollbar_state.get_position() + 1,
                                    active_nodes,
                                    external_nodes,
                                ));
                    }
                    Lists::Internal => {
                        self.internal_scrollbar_state =
                            self.internal_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.internal_scrollbar_state.get_position() + 1,
                                    active_nodes,
                                    external_nodes,
                                ));
                        actions.push(Action::DisplayLogs {
                            peer_id: active_nodes[node_idx],
                        });
                    }
                }
            }
            KeyCode::Down => {
                self.select_next();

                // Get the index of the newly selected node
                // Get the index of the newly selected node
                let node_idx = match self.node_list {
                    Lists::Internal => self.clamp(self.internal_list_state.selected().unwrap_or(0)),
                    Lists::External => self.clamp(self.external_list_state.selected().unwrap_or(0)),
                };

                match self.node_list {
                    Lists::External => {
                        self.external_scrollbar_state =
                            self.external_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.external_scrollbar_state.get_position() + 1,
                                    active_nodes,
                                    external_nodes,
                                ));
                    }
                    Lists::Internal => {
                        self.internal_scrollbar_state =
                            self.internal_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.internal_scrollbar_state.get_position() + 1,
                                    active_nodes,
                                    external_nodes,
                                ));
                        actions.push(Action::DisplayLogs {
                            peer_id: active_nodes[node_idx],
                        });
                    }
                }
            }
            KeyCode::Char('s') => match self.node_list {
                Lists::Internal => {
                    actions.push(Action::StartNode {
                        kad_mode: Mode::Server,
                    });
                }
                Lists::External => {}
            },
            KeyCode::Char('c') => match self.node_list {
                Lists::Internal => {
                    actions.push(Action::StartNode {
                        kad_mode: Mode::Client,
                    });
                }
                Lists::External => {}
            },
            KeyCode::Char('e') if !external_nodes.is_empty() => {
                self.node_list = match self.node_list {
                    Lists::Internal => Lists::External,
                    Lists::External => Lists::Internal,
                }
            }
            KeyCode::Enter => {
                let node_idx = match self.node_list {
                    Lists::Internal => self.clamp(self.internal_list_state.selected().unwrap_or(0)),
                    Lists::External => self.clamp(self.external_list_state.selected().unwrap_or(0)),
                };
                actions.push(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id: active_nodes[node_idx],
                });
            }
            _ => {}
        }

        Ok(())
    }

    pub fn select_next(&mut self) {
        match self.node_list {
            Lists::Internal => self.internal_list_state.select_next(),
            Lists::External => self.external_list_state.select_next(),
        }
    }

    pub fn select_previous(&mut self) {
        match self.node_list {
            Lists::Internal => self.internal_list_state.select_previous(),
            Lists::External => self.external_list_state.select_previous(),
        }
    }

    /// Moving up and down the listcan move past the bounds of the list,
    /// we must make sure it does not
    pub fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len { self.len - 1 } else { idx }
    }

    pub fn update(
        &mut self,
        action: &Action,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
    ) {
        match action {
            Action::AddNode { .. } => {
                self.len += 1;
                // Auto select the first node we add
                if self.internal_list_state.selected().is_none() {
                    self.internal_list_state = self.internal_list_state.with_selected(Some(0));

                    debug!(target: "app::node_list", "display log action added");

                    actions.push(Action::DisplayLogs {
                        peer_id: active_nodes[0],
                    });

                    debug!(target: "app::node_list", "display log action added");
                }
            }
            Action::RemoveNode { .. } => {
                self.len -= 1;
            }
            _ => {}
        }
    }
}

impl TuiEventHandler for NodeList {
    fn on_key(
        &mut self,
        key: KeyEvent,
        actions: &mut ActionQueue,
        ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(key, actions, ctx.active_nodes, ctx.external_nodes) {
            warn!(target: "tui::node_list", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(&mut self, action: &Action, actions: &mut ActionQueue, active: &IndexSet<PeerId>) {
        self.update(action, actions, active);
    }
}
