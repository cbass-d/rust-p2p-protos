use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock},
};

use color_eyre::eyre::{Context, Result};
use libp2p::PeerId;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument, trace};

use crate::{
    messages::{NetworkCommand, NetworkEvent},
    node::{NodeStats, history::MessageHistory},
    tui::{
        Tui, TuiEvent,
        components::{
            manage_connections::ManageConnections,
            node_box::NodeBox,
            node_commands::NodeCommands,
            node_info::NodeInfo,
            node_list::NodeList,
            node_log::NodeLog,
            popup::{PopUpContent, Popup},
        },
    },
};

/// Determines which of the TUI components is in focus and will take priority in handling user input
#[derive(Debug, PartialEq)]
pub enum Focus {
    NodeLog,
    NodeList,
    PopUp,
}

/// The action to perform as the result of handling user input or from a network event
#[derive(Debug, Clone)]
pub enum Action {
    /// Quit the Application
    Quit,

    /// Start a new node in the network
    StartNode,

    /// Add a newly started node to the TUI
    AddNode {
        peer_id: PeerId,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },

    /// Update connections in the TUI when a connection between two nodes is established
    UpdateConnections { peer_one: PeerId, peer_two: PeerId },

    /// Stop a node from running
    StopNode { peer_id: PeerId },

    /// Remove a stopped node from the TUI
    RemoveNode { peer_id: PeerId },

    /// Display the logs for the selected node
    DisplayLogs { peer_id: PeerId },

    /// Display the node commands popup for the selected node
    DisplayNodeCommands { peer_id: PeerId },

    /// Display the node info popup for the selected node
    DisplayInfo { peer_id: PeerId },

    /// Display the node identify info popup for the selected node
    DisplayIdentifyInfo { peer_id: PeerId },

    /// Display the node kademlia info popup for the selected node
    DisplayKademliaInfo { peer_id: PeerId },

    /// Connect peer_one to peer_two, peer_one being the initiator
    ConnectTo { peer_one: PeerId, peer_two: PeerId },

    /// Disconnect peer_one from peer_two, peer_one being the initiator
    DisconnectFrom { peer_one: PeerId, peer_two: PeerId },

    /// Display the manage connections popup for the selected node
    DisplayManageConnections { peer_id: PeerId },

    /// Close the node commands popup
    CloseNodeCommands,

    /// Display the Popup with the given content
    Popup {
        content: PopUpContent,
        peer_id: PeerId,
    },

    /// Get Identify info for node
    GetIdentifyInfo { peer_id: PeerId },
}

#[derive(Debug)]
pub struct App {
    /// Flag for stopping the TUI application
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

    /// Component for displaying the nodes on Canvas
    node_box: NodeBox,

    /// Component for displaying the swarm events from a node
    node_log: NodeLog,

    /// Component for displaying the list of active nodes
    node_list: NodeList,

    /// Component for displaying the list of active nodes
    popup: Popup,

    /// Queue of Actions to be performed
    actions: VecDeque<Action>,

    /// Determines which of the components is in focus
    focus: Focus,
}

impl App {
    /// Builds a new App structure, returns the App, a CancellationToken, mpsc sender for
    /// NetworkEvent, and a mpsc receiver for NetworkCommand
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
                popup: Popup::new(),
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

    /// The main logic for the TUI application
    #[instrument(skip_all, name = "TUI")]
    pub async fn run(&mut self) -> Result<()> {
        // Create and enter TUI terminal environment
        let mut tui = Tui::new()?.tick_rate(4.0).frame_rate(30.0);

        tui.enter()?;

        // Set the default focus to the NodeBox and NodeList component
        self.node_list.focus(true);
        self.node_box.focus(true);

        loop {
            tui.terminal.draw(|frame| self.render_frame(frame))?;

            let mut new_actions = VecDeque::new();

            // Select the next network event from the node network or TUI event from the
            // user/interface
            tokio::select! {
                Some(network_event) = self.network_event_rx.recv() => {

                    // Process the network event and get the action to preform
                    self.handle_network_event(network_event, &mut new_actions);

                }
                Some(tui_event) = tui.next() => {
                    self.handle_tui_event(tui_event, &mut new_actions).await;
                },
            }

            // Add any action that has created from the event streams
            self.actions.extend(new_actions);

            trace!(target: "TUI", "current actions: {:?}", self.actions);

            let mut current_actions = self.actions.clone();
            if !self.actions.is_empty() {
                self.process_actions(&mut current_actions).await;
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
            _ => {}
        }
    }

    /// Process an Action and update the state of the TUI application accordingly
    async fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        debug!(target: "TUI App", "updating TUI using action: {:?}", action);

        match action {
            Action::Quit => {
                self.quit = true;
            }
            Action::DisplayLogs { peer_id } => {
                if let Some(node_logs) = self.node_logs.get(&peer_id) {
                    debug!(target: "TUI App", "successfully got message history for node: {peer_id}");

                    self.node_log.display_logs(node_logs.clone());
                }
            }
            Action::DisplayNodeCommands { peer_id } => {
                debug!(target: "TUI App", "displaying node commands for {peer_id}");

                self.unfocus_list_graph();
            }
            Action::DisplayManageConnections { peer_id } => {
                debug!(target: "TUI App", "displaying manage connections for {peer_id}");

                self.unfocus_list_graph();
            }
            Action::DisplayInfo { peer_id } => {
                debug!(target: "TUI App", "displaying node info for {peer_id}");

                self.unfocus_list_graph();
            }
            Action::DisplayIdentifyInfo { peer_id } => {
                debug!(target: "TUI App", "displaying identify info for node {peer_id}");
                self.command_tx
                    .send(NetworkCommand::GetIdentifyInfo { peer_id })
                    .await
                    .unwrap();

                self.unfocus_list_graph();
            }
            Action::DisplayKademliaInfo { peer_id } => {
                debug!(target: "TUI App", "displaying kademlia info for node {peer_id}");
                self.command_tx
                    .send(NetworkCommand::GetKademliaInfo { peer_id })
                    .await
                    .unwrap();

                self.unfocus_list_graph();
            }
            Action::CloseNodeCommands => {
                self.focus_list_graph();
            }
            Action::ConnectTo { peer_one, peer_two } => {
                self.command_tx
                    .send(NetworkCommand::ConnectNodes { peer_one, peer_two })
                    .await
                    .unwrap();
            }
            Action::DisconnectFrom { peer_one, peer_two } => {
                self.command_tx
                    .send(NetworkCommand::DisconectNodes { peer_one, peer_two })
                    .await
                    .unwrap();
            }
            Action::StopNode { peer_id } => {
                self.command_tx
                    .send(NetworkCommand::StopNode { peer_id })
                    .await
                    .unwrap();
            }
            Action::RemoveNode { peer_id } => {
                self.focus_list_graph();
                self.actions.push_back(Action::CloseNodeCommands);
            }
            Action::StartNode => {
                self.command_tx
                    .send(NetworkCommand::StartNode)
                    .await
                    .unwrap();
            }
            Action::Popup { content, peer_id } => {
                self.popup.set_content(content);
                self.popup.set_node(peer_id);
                self.enable_popup();
            }
            _ => {}
        }

        // Pass the action through the different components adding the
        // returned action if needed
        self.node_box.update(action.clone(), actions);
        self.node_list.update(action.clone(), actions);
        self.node_log.update(action.clone(), actions);
        self.popup.update(action.clone(), actions);
    }

    fn enable_popup(&mut self) {
        self.focus = Focus::PopUp;
        self.unfocus_list_graph();
    }

    /// Process the current actions in the queue
    async fn process_actions(&mut self, actions: &mut VecDeque<Action>) {
        while let Some(action) = self.actions.pop_front() {
            self.update(action, actions).await;
        }

        self.actions = VecDeque::new();
    }

    /// Process a TUI event and output an Action
    async fn handle_tui_event(
        &mut self,
        event: TuiEvent,
        actions: &mut VecDeque<Action>,
    ) -> Option<Action> {
        match event {
            TuiEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                let _ = self
                    .handle_key_event(key_event, actions)
                    .await
                    .wrap_err_with(|| format!("handling key event failed: {key_event:#?}"));
            }
            _ => {}
        };

        match self.focus {
            Focus::NodeList => None,
            Focus::NodeLog => None,
            _ => None,
        }
    }

    /// Process a network event from the node network and output an Action
    fn handle_network_event(
        &mut self,
        network_event: NetworkEvent,
        actions: &mut VecDeque<Action>,
    ) {
        match network_event {
            NetworkEvent::NodeRunning {
                peer_id,
                message_history,
                node_connections,
            } => {
                debug!(target: "TUI", "network event recieved: node running");
                self.node_logs.insert(peer_id, message_history);

                actions.push_back(Action::AddNode {
                    peer_id: peer_id,
                    node_connections,
                });
            }
            NetworkEvent::NodeStopped { peer_id } => {
                debug!(target: "TUI", "network event recieved: node stopped");

                self.node_logs.remove_entry(&peer_id);

                actions.push_back(Action::RemoveNode { peer_id });
            }
            NetworkEvent::NodesConnected { peer_one, peer_two } => {
                debug!(target: "TUI", "updating connections between {} and {}", peer_one, peer_two);

                actions.push_back(Action::UpdateConnections { peer_one, peer_two });
            }
            NetworkEvent::IdentifyInfo { info } => {
                self.popup.identify_info.set_info(info);
            }
            NetworkEvent::KademliaInfo { info } => {
                self.popup.kademlia_info.set_info(info);
            }
        }
    }

    /// Process key input from the TUI
    async fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Tab => self.switch_focus(),
            _ => {}
        }

        match self.focus {
            Focus::NodeList => {
                self.node_box.handle_key_event(key_event, actions);
                self.node_list.handle_key_event(key_event, actions)?
            }
            Focus::NodeLog => self.node_log.handle_key_event(key_event, actions)?,
            Focus::PopUp => self.popup.handle_key_event(key_event, actions).await?,
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

        if self.focus == Focus::PopUp {
            trace!(target: "TUI", "rendering popup with content: {:?}", self.popup.content);

            let rect = Rect {
                x: frame.area().width / 4,
                y: frame.area().height / 3,
                width: frame.area().width / 2,
                height: frame.area().height / 3,
            };
            self.popup.render(frame, rect);
        }
        //} else if self.focus == Focus::ManageConnections {
        //    debug!(target: "TUI", "rendering manage connections");

        //    let rect = Rect {
        //        x: frame.area().width / 4,
        //        y: frame.area().height / 3,
        //        width: frame.area().width / 2,
        //        height: frame.area().height / 3,
        //    };
        //    self.manage_connections.render(frame, rect);
        //} else if self.focus == Focus::NodeInfo {
        //    debug!(target: "TUI", "rendering node info");

        //    let rect = Rect {
        //        x: frame.area().width / 4,
        //        y: frame.area().height / 3,
        //        width: frame.area().width / 2,
        //        height: frame.area().height / 3,
        //    };
        //    self.node_info.render(frame, rect);
        //}
    }

    pub fn exit(&mut self) {
        self.quit = true;
    }
}
