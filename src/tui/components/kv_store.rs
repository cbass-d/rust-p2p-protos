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

/// A single text-input field with its own cursor.
#[derive(Debug, Default)]
struct Field {
    buf: String,

    /// Byte index of the cursor into `buf`.
    cursor: usize,
}

impl Field {
    fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn delete_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = self.buf[..self.cursor]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        let new_pos = self.cursor - before;
        self.buf.remove(new_pos);
        self.cursor = new_pos;
    }

    fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = self.buf[..self.cursor]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor -= before;
    }

    fn move_right(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        let after = self.buf[self.cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor += after;
    }

    /// Column position of the cursor, in characters (for `set_cursor_position`).
    fn cursor_col(&self) -> u16 {
        self.buf[..self.cursor].chars().count() as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KvFocus {
    Key,
    Value,
}

impl KvFocus {
    fn toggle(self) -> Self {
        match self {
            KvFocus::Key => KvFocus::Value,
            KvFocus::Value => KvFocus::Key,
        }
    }
}

/// Popup for entering a key/value pair to store via Kademlia `PutRecord`.
#[derive(Debug)]
pub(crate) struct KvStore {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    key: Field,
    value: Field,

    /// Which field currently receives input.
    focus: KvFocus,
}

impl KvStore {
    pub(crate) fn new() -> Self {
        Self {
            node: None,
            key: Field::default(),
            value: Field::default(),
            focus: KvFocus::Key,
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    fn focused_field(&mut self) -> &mut Field {
        match self.focus {
            KvFocus::Key => &mut self.key,
            KvFocus::Value => &mut self.value,
        }
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
            KeyCode::Tab => self.focus = self.focus.toggle(),
            KeyCode::Char(c) => self.focused_field().insert_char(c),
            KeyCode::Backspace => self.focused_field().delete_char(),
            KeyCode::Left => self.focused_field().move_left(),
            KeyCode::Right => self.focused_field().move_right(),
            KeyCode::Enter => {
                actions.push(Action::PutRecord {
                    peer_id,
                    key: self.key.buf.clone(),
                    value: self.value.buf.clone(),
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

        let footer_text = Line::from(vec![
            Span::raw("<Esc> exit"),
            Span::raw(", "),
            Span::raw("<Tab> switch field"),
            Span::raw(", "),
            Span::raw("<Enter> submit"),
        ])
        .style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("KV Store")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // label, key input, label, value input.
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ]);
        let [key_label, key_area, value_label, value_area] = inner.layout(&layout);

        frame.render_widget(Paragraph::new(Text::from(Line::from("Key"))), key_label);
        frame.render_widget(Paragraph::new(Text::from(Line::from("Value"))), value_label);

        let key_inner = self.render_field(frame, key_area, &self.key, KvFocus::Key);
        let value_inner = self.render_field(frame, value_area, &self.value, KvFocus::Value);

        // Place the terminal cursor inside the focused field.
        let (inner_area, field) = match self.focus {
            KvFocus::Key => (key_inner, &self.key),
            KvFocus::Value => (value_inner, &self.value),
        };
        frame.set_cursor_position((inner_area.x + field.cursor_col(), inner_area.y));
    }

    /// Render one input field, returning the inner area where the cursor goes.
    fn render_field(&self, frame: &mut Frame, area: Rect, field: &Field, kind: KvFocus) -> Rect {
        let border_color = if self.focus == kind {
            Color::LightCyan
        } else {
            Color::DarkGray
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_color);
        let inner = input_block.inner(area);
        let input = Paragraph::new(field.buf.as_str())
            .style(Style::new().fg(Color::White))
            .block(input_block);
        frame.render_widget(input, area);
        inner
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
