use indexmap::IndexSet;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use color_eyre::eyre::Result;
use libp2p::{Multiaddr, PeerId};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument, trace, warn};

use crate::{
    error::AppError,
    messages::{
        IdentifyCommand, KademliaCommand, LifecycleCommand, NetworkCommand, NetworkEvent,
        SwarmCommand,
    },
    network::TransportMode,
    node::{NodeStats, history::MessageHistory},
    tui::{
        Tui, TuiEvent,
        action_queue::ActionQueue,
        components::{
            node_box::NodeBox,
            node_list::NodeList,
            node_log::NodeLog,
            popup::{PopUpContent, Popup},
        },
        event_handler::{TuiEventHandler, TuiKeyCtx},
    },
};

/// Determines which of the TUI components is in focus and will take priority in handling user input
#[derive(Debug, PartialEq)]
pub(crate) enum Focus {
    NodeLog,
    NodeList,
    PopUp,
}

/// The action to perform as the result of handling user input or from a network event
#[derive(Debug, Clone)]
pub(crate) enum Action {
    #[allow(dead_code)]
    /// Quit the Application
    Quit,

    /// Start a new node in the network
    StartNode,

    /// Add a newly started node to the TUI
    AddNode {
        peer_id: PeerId,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },

    /// Add an external node to the TUI
    AddExternalNode { peer_id: PeerId, address: Multiaddr },

    /// Update connections in the TUI when a connection between two nodes is established
    UpdateConnections { peer_one: PeerId, peer_two: PeerId },

    /// Stop a node from running
    StopNode { peer_id: PeerId },

    /// Remove a stopped node from the TUI
    RemoveNode { peer_id: PeerId },

    /// Display the logs for the selected node
    DisplayLogs { peer_id: PeerId },

    /// Display the node info popup for the selected node
    DisplayInfo { peer_id: PeerId },

    /// Display the node identify info popup for the selected node
    DisplayIdentifyInfo { peer_id: PeerId },

    /// Display the node kademlia info popup for the selected node
    DisplayKademliaInfo { peer_id: PeerId },

    /// Connect `peer_one` to `peer_two`, `peer_one` being the initiator
    ConnectTo { peer_one: PeerId, peer_two: PeerId },

    /// Disconnect `peer_one` from `peer_two`, `peer_one` being the initiator
    DisconnectFrom { peer_one: PeerId, peer_two: PeerId },

    /// Display the manage connections popup for the selected node
    DisplayManageConnections { peer_id: PeerId },

    /// Display the kv store popup for the selected node
    DisplayKvStore { peer_id: PeerId },

    /// Display the records list for local sotred KAD records
    DisplayRecordsList { peer_id: PeerId },

    /// Put reccord on the selected peer
    PutRecord {
        peer_id: PeerId,
        key: String,
        value: String,
    },

    /// Display the Popup with the given content
    Popup {
        content: PopUpContent,
        peer_id: PeerId,
    },

    /// Close the Popup
    ClosePopup,
}

#[derive(Debug)]
pub(crate) struct App {
    /// Flag for stopping the TUI application
    quit: bool,

    /// `CancellationToken` used to signal other running tasks
    /// to exit
    cancellation_token: CancellationToken,

    /// List of active nodes in the network
    active_nodes: IndexSet<PeerId>,

    /// List of active nodes in the network
    external_nodes: IndexSet<PeerId>,

    /// Broadcast subscription for events from the node network. A slow TUI
    /// receives `RecvError::Lagged(n)` rather than blocking the producers.
    network_event_rx: broadcast::Receiver<NetworkEvent>,

    /// MPSC channel for sending events from the TUI
    /// to the node network process
    network_command_tx: mpsc::Sender<NetworkCommand>,

    /// Message histories and stats of the nodes
    node_logs: HashMap<PeerId, (Arc<RwLock<MessageHistory>>, Arc<RwLock<NodeStats>>)>,

    /// Component for displaying the nodes on Canvas
    node_box: NodeBox,

    /// State of the current connections between the nodes
    node_connections: HashMap<PeerId, HashSet<PeerId>>,

    /// Component for displaying the swarm events from a node
    node_log: NodeLog,

    /// Component for displaying the list of active nodes
    node_list: NodeList,

    /// Component for displaying the list of active nodes
    popup: Popup,

    /// Queue of Actions to be performed
    actions: ActionQueue,

    /// Determines which of the components is in focus
    focus: Focus,

    // Tick rate for the TUI
    tick_rate: f64,

    // Frame rate for the TUI frame render
    frame_rate: f64,

    /// Transport mode the simulation is running under
    transport: TransportMode,
}

impl App {
    /// Builds a new `App` structure, returns the App, a `CancellationToken`, a broadcast
    /// receiver for `NetworkEvent`, and an mpsc sender for `NetworkCommand`
    pub fn new(
        cancellation_token: CancellationToken,
        network_event_rx: broadcast::Receiver<NetworkEvent>,
        network_command_tx: mpsc::Sender<NetworkCommand>,
        tick_rate: f64,
        frame_rate: f64,
        transport: TransportMode,
    ) -> Self {
        Self {
            quit: false,
            cancellation_token,
            network_event_rx,
            network_command_tx,
            node_box: NodeBox::new(),
            node_log: NodeLog::new(),
            node_list: NodeList::new(),
            popup: Popup::new(),
            actions: ActionQueue::default(),
            focus: Focus::NodeList,
            node_logs: HashMap::new(),
            active_nodes: IndexSet::default(),
            external_nodes: IndexSet::default(),
            node_connections: HashMap::default(),
            tick_rate,
            frame_rate,
            transport,
        }
    }

    /// The main logic for the TUI application
    #[instrument(skip_all, name = "app")]
    pub async fn run(&mut self) -> Result<(), AppError> {
        // Create and enter TUI terminal environment
        let mut tui = Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);

        tui.enter()?;

        // Set the default focus to the NodeBox and NodeList component
        self.node_list.focus(true);
        self.node_box.focus(true);

        loop {
            tui.terminal
                .draw(|frame| self.render_frame(frame))
                .map_err(AppError::TerminalDraw)?;

            // Select the next network event from the node network or TUI event from the
            // user/interface
            tokio::select! {
                result = self.network_event_rx.recv() => {
                    match result {
                        Ok(network_event) => self.handle_network_event(network_event),
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: "TUI", skipped = n, "event bus lagged — TUI missed events");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: "TUI", "event bus closed, exiting");
                            self.quit = true;
                        }
                    }
                }
                Some(tui_event) = tui.next() => {
                    self.handle_tui_event(tui_event).await;
                },
            }

            trace!(target: "TUI", "current actions: {:?}", self.actions);
            self.process_actions().await;

            if self.quit {
                break;
            }
        }

        self.cancellation_token.cancel();

        debug!(target: "TUI", "TUI CANCELLED TOKEN");
        tui.exit().await?;

        Ok(())
    }

    fn focus_list_graph(&mut self) {
        self.node_list.focus(true);
        self.node_box.focus(true);

        self.focus = Focus::NodeList;
    }

    fn unfocus_list_graph(&mut self) {
        self.node_list.focus(false);
        self.node_box.focus(false);
    }

    fn focus_logs(&mut self) {
        self.node_log.focus(true);

        self.focus = Focus::NodeLog;
    }

    fn unfocus_logs(&mut self) {
        self.node_log.focus(false);
    }

    /// Switch focus between the two main TUI components
    pub fn switch_focus(&mut self) {
        match self.focus {
            Focus::NodeList => {
                self.unfocus_list_graph();
                self.focus_logs();
            }
            Focus::NodeLog => {
                self.focus_list_graph();
                self.unfocus_logs();
            }
            Focus::PopUp => {}
        }
    }

    /// Process an Action and update the state of the TUI application accordingly
    async fn update(&mut self, action: Action) {
        debug!(target: "app::actions", "updating TUI using action: {:?}", action);

        match action {
            Action::Quit => {
                self.quit = true;
            }
            Action::DisplayLogs { peer_id } => {
                if let Some(node_logs) = self.node_logs.get(&peer_id) {
                    debug!(target: "app", "successfully got message history for node: {peer_id}");

                    let (messages, stats) = node_logs;
                    self.node_log
                        .display_logs((messages.clone(), stats.clone()));
                }
            }
            Action::DisplayManageConnections { peer_id } => {
                debug!(target: "app", "displaying manage connections for {peer_id}");

                self.unfocus_list_graph();
            }
            Action::DisplayInfo { peer_id } => {
                debug!(target: "app", "displaying node info for {peer_id}");

                self.unfocus_list_graph();
            }
            Action::DisplayIdentifyInfo { peer_id } => {
                debug!(target: "app", "displaying identify info for node {peer_id}");
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Identify(IdentifyCommand::GetInfo {
                        peer_id,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_id, "tui sent GetIdentifyInfo network command");

                self.unfocus_list_graph();
            }
            Action::DisplayKademliaInfo { peer_id } => {
                debug!(target: "app", "displaying kademlia info for node {peer_id}");
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Kademlia(KademliaCommand::GetInfo {
                        peer_id,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_id, "tui sent GetKademliaInfo network command");

                self.unfocus_list_graph();
            }
            Action::DisplayRecordsList { peer_id } => {
                debug!(target: "app", "displaying records list for node {peer_id}");
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Kademlia(KademliaCommand::RecordsList {
                        peer_id,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_id, "tui sent RecordsList network command");

                self.unfocus_list_graph();
            }
            Action::DisplayKvStore { peer_id } => {
                debug!(target: "app", "displaying kv store popup for node {peer_id}");

                self.unfocus_list_graph();
            }
            Action::ConnectTo { peer_one, peer_two } => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Swarm(SwarmCommand::Connect {
                        peer_one,
                        peer_two,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_one, %peer_two, "tui sent ConnectTo network command");
            }
            Action::AddExternalNode {
                peer_id,
                ref address,
            } => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Lifecycle(LifecycleCommand::AddExternal {
                        peer_id,
                        address: address.clone(),
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }
            }
            Action::PutRecord {
                peer_id,
                ref key,
                ref value,
            } => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Kademlia(KademliaCommand::PutRecord {
                        peer_id,
                        key: key.clone(),
                        value: value.clone(),
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_id, "tui sent PutRecord network command");
            }
            Action::DisconnectFrom { peer_one, peer_two } => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Swarm(SwarmCommand::Disconnect {
                        peer_one,
                        peer_two,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_one, %peer_two, "tui sent DisconnectFrom network command");
            }
            Action::StopNode { peer_id } => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Lifecycle(LifecycleCommand::Stop {
                        peer_id,
                    }))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", %peer_id, "tui sent StopNode network command");
            }
            Action::RemoveNode { .. } => {
                self.focus_list_graph();
                self.actions.push(Action::ClosePopup);
            }
            Action::StartNode => {
                if let Err(e) = self
                    .network_command_tx
                    .send(NetworkCommand::Lifecycle(LifecycleCommand::Start))
                    .await
                {
                    warn!(target: "app", "failed to send network command: {e}");
                }

                debug!(target: "app", "tui sent StartNode network command");
            }
            Action::ClosePopup => {
                self.close_popup();
            }
            Action::Popup { content, peer_id } => {
                self.popup.set_content(content);
                self.popup.set_node(peer_id);
                self.open_popup();
            }
            _ => {}
        }

        // Fan out to components; each self-filters.
        self.node_box
            .on_action(&action, &mut self.actions, &self.active_nodes);
        self.node_list
            .on_action(&action, &mut self.actions, &self.active_nodes);
        self.node_log
            .on_action(&action, &mut self.actions, &self.active_nodes);
        self.popup
            .on_action(&action, &mut self.actions, &self.active_nodes);
    }

    fn close_popup(&mut self) {
        self.focus_list_graph();
    }

    fn open_popup(&mut self) {
        self.focus = Focus::PopUp;
        self.unfocus_list_graph();
    }

    /// Process the current actions in the queue
    async fn process_actions(&mut self) {
        while let Some(action) = self.actions.next() {
            debug!(target: "app::actions", action = ?action, "processing action");
            self.update(action).await;
        }
    }

    /// Process a TUI event and output an Action
    async fn handle_tui_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                if let Err(e) = self.handle_key_event(key_event).await {
                    warn!(target: "app", error = %e, "failed to handle TUI key event: {e}");
                }
            }
            _ => {}
        }
    }

    /// Update cross-cutting app state, then fan out to components.
    fn handle_network_event(&mut self, network_event: NetworkEvent) {
        self.update_app_state_from_event(&network_event);
        self.dispatch_network_event(&network_event);
    }

    /// Cross-cutting state updates owned by App, not any single component.
    fn update_app_state_from_event(&mut self, event: &NetworkEvent) {
        match event {
            NetworkEvent::NodeRunning {
                peer_id,
                message_history,
                stats,
                node_connections,
            } => {
                debug!(target: "TUI", "network event received: node running");
                self.node_logs
                    .insert(*peer_id, (message_history.clone(), stats.clone()));
                self.active_nodes.insert(*peer_id);
                self.node_connections.insert(*peer_id, HashSet::new());
                self.actions.push(Action::AddNode {
                    peer_id: *peer_id,
                    node_connections: node_connections.clone(),
                });
            }
            NetworkEvent::NodeStopped { peer_id } => {
                debug!(target: "TUI", "network event received: node stopped");
                self.node_logs.remove_entry(peer_id);
                self.active_nodes.swap_remove(peer_id);
                self.actions.push(Action::RemoveNode { peer_id: *peer_id });
            }
            NetworkEvent::NodesConnected { peer_one, peer_two } => {
                debug!(target: "TUI", "updating connections between {} and {}", peer_one, peer_two);
                self.node_connections
                    .entry(*peer_one)
                    .or_default()
                    .insert(*peer_two);
                self.node_connections
                    .entry(*peer_two)
                    .or_default()
                    .insert(*peer_one);
                self.actions.push(Action::UpdateConnections {
                    peer_one: *peer_one,
                    peer_two: *peer_two,
                });
            }
            NetworkEvent::NodeDiscovered { peer_id } => {
                debug!(target: "TUI", "adding external node to TUI {}", peer_id);
                let already_added =
                    self.external_nodes.contains(peer_id) || self.active_nodes.contains(peer_id);
                if !already_added {
                    self.external_nodes.insert(*peer_id);
                    self.actions.push(Action::AddExternalNode {
                        peer_id: *peer_id,
                        address: Multiaddr::empty(),
                    });
                }
            }
            NetworkEvent::NodesDisconnected { peer_one, peer_two } => {
                if let Some(set) = self.node_connections.get_mut(peer_one) {
                    set.remove(peer_two);
                }
                if let Some(set) = self.node_connections.get_mut(peer_two) {
                    set.remove(peer_one);
                }
            }
            NetworkEvent::NodeListening { .. }
            | NetworkEvent::NodeExpired { .. }
            | NetworkEvent::MaxNodes => {}

            NetworkEvent::IdentifyInfo { .. }
            | NetworkEvent::KademliaInfo { .. }
            | NetworkEvent::RecordsList { .. } => {}
        }
    }

    /// Fan out to every component's `on_network_event`. Components self-filter.
    fn dispatch_network_event(&mut self, event: &NetworkEvent) {
        self.node_box.on_network_event(event, &mut self.actions);
        self.node_list.on_network_event(event, &mut self.actions);
        self.node_log.on_network_event(event, &mut self.actions);
        self.popup.on_network_event(event, &mut self.actions);
    }

    /// Dispatch key input through the focused component chain.
    async fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Tab => self.switch_focus(),
            _ => {}
        }

        let ctx = TuiKeyCtx {
            active_nodes: &self.active_nodes,
            external_nodes: &self.external_nodes,
            transport_tcp: self.transport == TransportMode::Tcp,
        };

        match self.focus {
            Focus::NodeList => {
                let _ = self.node_list.on_key(key_event, &mut self.actions, &ctx);
                let _ = self.node_box.on_key(key_event, &mut self.actions, &ctx);
            }
            Focus::NodeLog => {
                let _ = self.node_log.on_key(key_event, &mut self.actions, &ctx);
            }
            Focus::PopUp => {
                let _ = self.popup.on_key(key_event, &mut self.actions, &ctx);
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

        self.node_box
            .render(frame, top_chunks[0], &self.active_nodes);
        self.node_list.render(
            frame,
            top_chunks[1],
            &self.active_nodes,
            &self.external_nodes,
        );
        self.node_log
            .render(frame, main_chunks[1], &self.active_nodes);

        if self.focus == Focus::PopUp {
            trace!(target: "app", "rendering popup with content: {:?}", self.popup.content);

            let rect = Rect {
                x: frame.area().width / 4,
                y: frame.area().height / 3,
                width: frame.area().width / 2,
                height: frame.area().height / 2,
            };
            self.popup.render(
                frame,
                rect,
                &self.active_nodes,
                &self.external_nodes,
                &self.node_connections,
                self.transport == TransportMode::Tcp,
            );
        }
    }

    pub fn exit(&mut self) {
        self.quit = true;
    }
}
