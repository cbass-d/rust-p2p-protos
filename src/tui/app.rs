use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock},
    time::Duration,
};

use color_eyre::eyre::{Context, Result};
use libp2p::PeerId;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument};

use crate::{
    messages::{NetworkCommand, NetworkEvent},
    node::{NodeStats, history::MessageHistory},
    tui::{
        Tui, TuiEvent,
        components::{node_box::NodeBox, node_list::NodeList, node_log::NodeLog},
    },
};

/// Determines which of the TUI components
/// is in focus and will take priority in handling user input
#[derive(Debug)]
pub enum Focus {
    NodeLog,
    NodeList,
}

/// The action which is the result of handling user input
#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    AddNode {
        peer_id: PeerId,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },
    AddNodeToGraph((PeerId, Arc<RwLock<HashSet<PeerId>>>)),
    RemoveNode(PeerId),
    DisplayLogs(PeerId),
}

#[derive(Debug)]
pub struct App {
    quit: bool,

    /// CancellationToken used to signal other running tasks
    /// to exit
    cancellation_token: CancellationToken,

    /// MPSC channel for recieving events from the node network to be
    /// reflected by the TUI
    network_event_rx: mpsc::Receiver<NetworkEvent>,

    /// MPSC channel for sending events from the TUI
    /// to the node network process
    command_tx: mpsc::Sender<NetworkCommand>,

    /// Message histories and stats of the nodes
    node_logs: HashMap<PeerId, Arc<RwLock<(MessageHistory, NodeStats)>>>,

    /// HashMap containg the connections between the nodes
    node_connections: HashMap<PeerId, Arc<RwLock<HashSet<PeerId>>>>,

    /// TUI components
    node_box: NodeBox,
    node_log: NodeLog,
    node_list: NodeList,

    /// Queue of Actions to be performed
    actions: VecDeque<Action>,

    /// Determines which of the components is in focus
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
                node_log: NodeLog::new(),
                node_list: NodeList::new(),
                actions: VecDeque::new(),
                focus: Focus::NodeList,
                node_logs: HashMap::new(),
                node_connections: HashMap::new(),
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

        // Set the default focus to the NodeBox component
        self.node_list.focus(true);

        loop {
            tui.terminal.draw(|frame| self.render_frame(frame))?;

            // Select the next network event from the node network or TUI event from the
            // user/interface
            tokio::select! {
                Some(network_event) = self.network_event_rx.recv() => {

                    // Process the network event and get the action to preform
                    let maybe_action = self.handle_network_event(network_event);

                    debug!(target: "TUI", "Network event action being done: {:?}", maybe_action);

                    if let Some(action) = maybe_action {
                        self.update(action);
                        self.process_actions();
                    }

                }
                Some(tui_event) = tui.next() => {
                    let maybe_action = self.handle_tui_event(tui_event);

                    debug!(target: "TUI", "TUI action being done: {:?}", maybe_action);

                    if let Some(action) = maybe_action {
                        self.update(action);
                        self.process_actions();
                    }
                },
            }

            if self.quit {
                break;
            }
        }

        self.cancellation_token.cancel();

        debug!(target: "TUI", "TUI CANCELLED TOKEN");
        tui.exit()?;

        Ok(())
    }

    pub fn switch_focus(&mut self) {
        match self.focus {
            Focus::NodeList => {
                self.node_list.focus(false);
                self.node_log.focus(true);
                self.focus = Focus::NodeLog;
            }
            Focus::NodeLog => {
                self.node_list.focus(true);
                self.node_log.focus(false);
                self.focus = Focus::NodeList;
            }
        }
    }

    /// Process an Action and update the state of the TUI application
    /// accordingly
    fn update(&mut self, action: Action) {
        debug!(target: "TUI App", "updating TUI using action: {:?}", action);

        match action {
            Action::Quit => {
                self.quit = true;
            }
            Action::DisplayLogs(peer) => {
                if let Some(node_logs) = self.node_logs.get(&peer) {
                    debug!(target: "TUI App", "successfully got message history for node: {peer}");

                    self.node_log.display_logs(node_logs.clone());
                }
            }
            _ => {}
        }

        // Pass the action through the different components adding the
        // returned action if needed
        if let Some(action) = self.node_box.update(action.clone()) {
            self.actions.push_back(action);
        }

        if let Some(action) = self.node_list.update(action.clone()) {
            self.actions.push_back(action);
        }

        if let Some(action) = self.node_log.update(action) {
            self.actions.push_back(action);
        }
    }

    /// Process the current actions in the queue
    fn process_actions(&mut self) {
        while let Some(action) = self.actions.pop_front() {
            self.update(action);
        }
    }

    /// Process a TUI event and output an Action
    fn handle_tui_event(&mut self, event: TuiEvent) -> Option<Action> {
        match event {
            TuiEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                let _ = self
                    .handle_key_event(key_event)
                    .wrap_err_with(|| format!("handling key event failed: {key_event:#?}"));
            }
            _ => {}
        };

        match self.focus {
            Focus::NodeList => None,
            Focus::NodeLog => None,
        }
    }

    /// Process a network event from the node network and output an Action
    fn handle_network_event(&mut self, network_event: NetworkEvent) -> Option<Action> {
        match network_event {
            NetworkEvent::NodeRunning((peer, node_logs, node_connections)) => {
                debug!(target: "TUI", "network event recieved: node running");
                self.node_logs.insert(peer, node_logs);

                Some(Action::AddNode {
                    peer_id: peer,
                    node_connections,
                })
            }
            NetworkEvent::NodeStopped(peer) => {
                debug!(target: "TUI", "network event recieved: node stopped");

                self.node_logs.remove_entry(&peer);
                Some(Action::RemoveNode(peer))
            }
            _ => None,
        }
    }

    /// Process key input from the TUI
    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Tab => self.switch_focus(),
            _ => {}
        }

        match self.focus {
            Focus::NodeList => {
                if let Some(action) = self.node_box.handle_key_event(key_event) {
                    debug!(target: "TUI", "new action recieved from node box key event: {:?}", action);
                    self.update(action);
                }

                if let Some(action) = self.node_list.handle_key_event(key_event) {
                    debug!(target: "TUI", "new action recieved from node box key event: {:?}", action);
                    self.update(action);
                }
            }
            Focus::NodeLog => {
                if let Some(action) = self.node_log.handle_key_event(key_event) {
                    debug!(target: "TUI", "new action recieved from node box key event: {:?}", action);
                    self.update(action);
                }
            }
        }

        Ok(())
    }

    pub fn render_frame(&mut self, frame: &mut Frame) {
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(frame.area());

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(main_chunks[0]);

        self.node_box.render(frame, top_chunks[0]);
        self.node_list.render(frame, top_chunks[1]);
        self.node_log.render(frame, main_chunks[1]);
    }

    pub fn exit(&mut self) {
        self.quit = true;
    }
}
