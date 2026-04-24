use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget},
};

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    components::popup::PopUpContent,
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

/// Popup for entering a key to store via Kademlia `PutRecord`.
#[derive(Debug)]
pub(crate) struct KvStore {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    key_input: String,

    /// Byte index of the cursor into `key_input`
    cursor_pos: usize,
}

impl KvStore {
    pub(crate) fn new() -> Self {
        Self {
            node: None,
            key_input: String::default(),
            cursor_pos: 0,
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    fn insert_char(&mut self, c: char) {
        self.key_input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    fn delete_char(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let before = self.key_input[..self.cursor_pos]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        let new_pos = self.cursor_pos - before;
        self.key_input.remove(new_pos);
        self.cursor_pos = new_pos;
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let before = self.key_input[..self.cursor_pos]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor_pos -= before;
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_pos >= self.key_input.len() {
            return;
        }
        let after = self.key_input[self.cursor_pos..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor_pos += after;
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(&mut self, key_event: KeyEvent, actions: &mut ActionQueue) {
        let Some(peer_id) = self.node else {
            return;
        };

        match key_event.code {
            KeyCode::Esc => {
                actions.push(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id,
                });
            }
            KeyCode::Char(c) => self.insert_char(c),
            KeyCode::Backspace => self.delete_char(),
            KeyCode::Left => self.move_cursor_left(),
            KeyCode::Right => self.move_cursor_right(),
            KeyCode::Enter => {
                actions.push(Action::PutRecord {
                    peer_id,
                    key: self.key_input.clone(),
                    value: String::default(),
                });

                actions.push(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id,
                });
            }
            _ => {}
        }
    }

    /// Render the `KvStore` component with the current context
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        if self.node.is_none() {
            return;
        }

        let footer_text =
            Line::from(vec![Span::raw("<Esc> exit")]).style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("KV Store")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        // Compute the inner area (inside the border/padding) before rendering the block
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Lay out the inner area for the label and input
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(3)]);
        let [label_area, input_area] = inner.layout(&layout);

        let label = Paragraph::new(Text::from(Line::from("Key to store")));
        frame.render_widget(label, label_area);

        let input_block = Block::default().borders(Borders::ALL);
        let input_inner = input_block.inner(input_area);
        let input = Paragraph::new(self.key_input.as_str())
            .style(Style::new().fg(Color::White))
            .block(input_block);
        frame.render_widget(input, input_area);

        // Position the cursor inside the input area
        let cursor_col = self.key_input[..self.cursor_pos].chars().count() as u16;
        frame.set_cursor_position((input_inner.x + cursor_col, input_inner.y));
    }
}

impl TuiEventHandler for KvStore {
    fn on_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        actions: &mut ActionQueue,
        _ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        self.handle_key_event(key, actions);
        KeyResult::Handled
    }
}
