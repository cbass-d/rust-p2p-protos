use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListState, Padding, Widget},
};
use tracing::debug;

use indexmap::IndexSet;
use tracing::warn;

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

/// Top-level popup menu for a selected node.
#[derive(Debug)]
pub(crate) struct NodeCommands {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// The length of the List
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,
}

impl NodeCommands {
    pub fn new() -> Self {
        Self {
            node: None,
            len: 5,
            list_state: ListState::default().with_selected(Some(0)),
        }
    }

    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
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
        if idx >= self.len { self.len - 1 } else { idx }
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut ActionQueue,
    ) -> Result<()> {
        if let Some(peer_id) = self.node {
            match key_event.code {
                KeyCode::Up => {
                    self.select_previous();
                }
                KeyCode::Down => {
                    self.select_next();
                }
                KeyCode::Enter => {
                    let selection = self.clamp(self.list_state.selected().unwrap_or(0));

                    debug!(target: "app::node_commands", "node commands option {} selected", selection);

                    match selection {
                        0 => {
                            actions.push(Action::DisplayManageConnections { peer_id });
                        }
                        1 => {
                            actions.push(Action::DisplayInfo { peer_id });
                        }
                        2 => {
                            actions.push(Action::DisplayKvStore { peer_id });
                        }
                        3 => {
                            actions.push(Action::DisplayRecordsList { peer_id });
                        }
                        4 => {
                            actions.push(Action::StopNode { peer_id });
                        }
                        _ => {}
                    }
                }
                KeyCode::Esc => {
                    actions.push(Action::ClosePopup);
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        let footer_text = Line::from(vec![
            Span::raw("<Esc> exit"),
            Span::raw(", "),
            Span::raw("<Up> <Down> select options"),
        ])
        .style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("Node Commands")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        if self.node.is_none() {
            return;
        }

        let list_items = vec![
            "Manage Connections",
            "Node Info",
            "KV Store",
            "KAD Records List",
            "Remove Node (stop node)",
        ];

        let list = List::new(list_items)
            .highlight_style(Style::new().reversed())
            .highlight_symbol("*")
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn update(&mut self, _action: &Action, _actions: &mut ActionQueue) {}
}

impl TuiEventHandler for NodeCommands {
    fn on_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        actions: &mut ActionQueue,
        _ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(key, actions) {
            warn!(target: "tui::node_commands", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(
        &mut self,
        action: &Action,
        actions: &mut ActionQueue,
        _active: &IndexSet<PeerId>,
    ) {
        self.update(action, actions);
    }
}
