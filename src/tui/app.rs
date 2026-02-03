use color_eyre::eyre::{Context, Result};
use libp2p::PeerId;
use ratatui::{
    Frame,
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument};

use crate::{
    messages::{NetworkCommand, NetworkEvent},
    tui::{Tui, TuiEvent, components::{node_box::NodeBox, node_graph::NodeGraph}},
};

/// Determines which of the TUI components
/// is in focus and will take priority in handling user input
#[derive(Debug)]
pub enum Focus {
    NodeBox,
    NodeGraph,
}

/// The action which is the result of handling user input
#[derive(Debug)]
pub enum Action {
    Quit,
    AddNode(PeerId),
    RemoveNode(PeerId),
}

#[derive(Debug)]
pub struct App {
    quit: bool,

    /// CancellationToken used to signal other running tasks
    /// to exit
    cancellation_token: CancellationToken,

    // MPSC channel for recieving events from the node network to be
    // reflected by the TUI
    network_event_rx: mpsc::Receiver<NetworkEvent>,

    // MPSC channel for sending events from the TUI
    // to the node network process
    command_tx: mpsc::Sender<NetworkCommand>,

    /// TUI components
    node_box: NodeBox,
    node_graph: NodeGraph,

    focus: Focus,
}

impl App {
    pub fn new() -> (
        Self,
        CancellationToken,
        mpsc::Sender<NetworkEvent>,
        mpsc::Receiver<NetworkCommand>,
    ) {
        // Create the MPSC channels for passing of messages betweeen the node network and the TUI
        let (network_event_tx, network_event_rx) = mpsc::channel::<NetworkEvent>(10);
        let (command_tx, command_rx) = mpsc::channel::<NetworkCommand>(10);
        let cancellation_token = CancellationToken::new();
        (
            Self {
                quit: false,
                cancellation_token: cancellation_token.clone(),
                network_event_rx,
                command_tx,
                node_box: NodeBox::new(),
                node_graph: NodeGraph::new(),
                focus: Focus::NodeBox,
            },
            cancellation_token,
            network_event_tx,
            command_rx,
        )
    }

    /// The main logic for the TUI
    #[instrument(skip_all, name = "TUI")]
    pub async fn run(&mut self) -> Result<()> {
        // Create and enter TUI terminal environment
        let mut tui = Tui::new()?.tick_rate(4.0).frame_rate(30.0);
        tui.enter()?;

        loop {
            tui.terminal.draw(|frame| self.render_frame(frame))?;

            // Select network event from the node network or TUI event from the
            // user/interface
            tokio::select! {
                Some(network_event) = self.network_event_rx.recv() => {

                    // Process the network event and get the action to preform
                    let maybe_action = self.handle_network_event(network_event);
                    debug!(target: "TUI", "action being done: {:?}", maybe_action);

                    // Perform the action
                    if let Some(action) = maybe_action {
                        self.update(action);
                    }
                }
                Some(tui_event) = tui.next() => {
                    let maybe_action = self.handle_tui_event(tui_event);
                    if let Some(action) = maybe_action {
                        self.update(action);
                    }
                },
            }

            if self.quit {
                break;
            }
        }

        tui.exit()?;

        Ok(())
    }

    /// Process an Action and update the state of the TUI application
    /// accordingly
    fn update(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.quit = true;
            }
            _ => {}
        }

        self.node_box.update(action);
    }

    /// Process a TUI event and output an Action
    fn handle_tui_event(&mut self, event: TuiEvent) -> Option<Action> {
        match event {
            TuiEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                let _ = self
                    .handle_key_event(key_event)
                    .wrap_err_with(|| format!("handling key event failed: {key_event:#?}"));

                Some(Action::Quit)
            }
            _ => None,
        };

        match self.focus {
            Focus::NodeBox => None,
            Focus::NodeGraph => None,
        }
    }

    /// Process a network event from the node network and output an Action
    fn handle_network_event(&mut self, network_event: NetworkEvent) -> Option<Action> {
        match network_event {
            NetworkEvent::NodeRunning(peer) => {
                debug!(target: "TUI", "network event recieved: node running");
                Some(Action::AddNode(peer))
            }
            NetworkEvent::NodeStopped(peer) => {
                debug!(target: "TUI", "network event recieved: node stopped");
                Some(Action::RemoveNode(peer))
            }
        }
    }

    /// Process key input from the TUI
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            _ => {}
        }

        match self.focus {
            Focus::NodeBox => {
                let res = self.node_box.handle_key_event(key_event);
            }
            Focus::NodeGraph => {}
        }

        Ok(())
    }

    pub fn render_frame(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(frame.area());

        self.node_box.render(frame, chunks[1]);
        self.node_graph.render(frame, chunks[0]);
    }

    pub fn exit(&mut self) {
        self.quit = true;
        self.cancellation_token.cancel();
    }
}
