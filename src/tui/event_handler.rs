//! Uniform contract for TUI components.

use crossterm::event::KeyEvent;
use indexmap::IndexSet;
use libp2p::PeerId;

use crate::{
    messages::NetworkEvent,
    tui::{action_queue::ActionQueue, app::Action},
};

/// Outcome of a key dispatch; `Handled` short-circuits further components.
pub(crate) enum KeyResult {
    Handled,
    NotHandled,
}

/// Read-only app state handed to a component during `on_key`.
pub(crate) struct TuiKeyCtx<'a> {
    pub active_nodes: &'a IndexSet<PeerId>,
    pub external_nodes: &'a IndexSet<PeerId>,
    pub transport_tcp: bool,
}

/// Implemented by every TUI component; all methods default to no-ops.
pub(crate) trait TuiEventHandler {
    fn on_network_event(&mut self, _event: &NetworkEvent, _actions: &mut ActionQueue) {}

    fn on_action(
        &mut self,
        _action: &Action,
        _actions: &mut ActionQueue,
        _active_nodes: &IndexSet<PeerId>,
    ) {
    }

    fn on_key(
        &mut self,
        _key: KeyEvent,
        _actions: &mut ActionQueue,
        _ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        KeyResult::NotHandled
    }
}
