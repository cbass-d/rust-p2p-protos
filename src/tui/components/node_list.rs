use color_eyre::eyre::Result;
use indexmap::IndexSet;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tracing::debug;

use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::tui::{app::Action, components::popup::PopUpContent};

#[derive(Debug, Clone)]
pub(crate) enum Lists {
    Internal,
    External,
}

/// A display for the currently active nodes
/// Consists of a list that can be iterated through by
/// the user
#[derive(Debug, Clone)]
pub(crate) struct NodeList {
    /// Hashset containig the list of active nodes, shared by the App
    /// and other components
    active_nodes: Arc<RwLock<IndexSet<PeerId>>>,

    /// Hashset containig the list of external nodes, shared by the App
    /// and other components
    external_nodes: Arc<RwLock<IndexSet<PeerId>>>,

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
    pub fn new(
        active_nodes: Arc<RwLock<IndexSet<PeerId>>>,
        external_nodes: Arc<RwLock<IndexSet<PeerId>>>,
    ) -> Self {
        Self {
            active_nodes,
            external_nodes,
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

    fn clap_scrollbar_pos(&self, new_pos: usize) -> usize {
        let len = match self.node_list {
            Lists::Internal => self.active_nodes.read().len(),
            Lists::External => self.external_nodes.read().len(),
        };
        if new_pos > len { len } else { new_pos }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let external_nodes = self.external_nodes.read();
        let active_nodes = self.active_nodes.read();

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
            block = block.title_bottom("<a> Add to add node <e> Switch to external");
        } else if active_nodes.len() < 10 && external_nodes.is_empty() {
            block = block.title_bottom("<a> Add to add node");
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
            .content_length(self.active_nodes.read().len());
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
                .content_length(self.external_nodes.read().len());
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
        actions: &mut VecDeque<Action>,
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
                                ));
                    }
                    Lists::Internal => {
                        self.internal_scrollbar_state =
                            self.internal_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.internal_scrollbar_state.get_position() + 1,
                                ));
                        let active_nodes = self.active_nodes.read();
                        actions.push_back(Action::DisplayLogs {
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
                                ));
                    }
                    Lists::Internal => {
                        self.internal_scrollbar_state =
                            self.internal_scrollbar_state
                                .position(self.clap_scrollbar_pos(
                                    self.internal_scrollbar_state.get_position() + 1,
                                ));
                        let active_nodes = self.active_nodes.read();
                        actions.push_back(Action::DisplayLogs {
                            peer_id: active_nodes[node_idx],
                        });
                    }
                }
            }
            KeyCode::Char('a') => {
                actions.push_back(Action::StartNode);
            }
            KeyCode::Char('e') => {
                if !self.external_nodes.read().is_empty() {
                    self.node_list = match self.node_list {
                        Lists::Internal => Lists::External,
                        Lists::External => Lists::Internal,
                    }
                }
            }
            KeyCode::Enter => {
                let node_idx = match self.node_list {
                    Lists::Internal => self.clamp(self.internal_list_state.selected().unwrap_or(0)),
                    Lists::External => self.clamp(self.external_list_state.selected().unwrap_or(0)),
                };
                let active_nodes = self.active_nodes.read();
                actions.push_back(Action::Popup {
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

    pub fn update(&mut self, action: &Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::AddNode { .. } => {
                self.len += 1;
                // Auto select the first node we add
                if self.internal_list_state.selected().is_none() {
                    self.internal_list_state = self.internal_list_state.with_selected(Some(0));

                    debug!(target: "app::node_list", "display log action added");

                    let active_nodes = self.active_nodes.read();
                    actions.push_back(Action::DisplayLogs {
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
