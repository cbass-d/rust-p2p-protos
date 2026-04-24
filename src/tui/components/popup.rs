use std::collections::{HashMap, HashSet};

use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{Frame, layout::Rect};

use tracing::warn;

use crate::tui::{
    action_queue::ActionQueue,
    app::Action,
    components::{
        identify_info::IdentifyInfo, kademlia_info::KademliaInfo, kv_store::KvStore,
        manage_connections::ManageConnections, node_commands::NodeCommands, node_info::NodeInfo,
    },
    event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
};

/// What content/component should the Popup display
#[derive(Debug, Clone, Copy)]
pub(crate) enum PopUpContent {
    NodeCommands,
    ManageConnections,
    NodeInfo,
    IdentifyInfo,
    KademliaInfo,
    KvStore,
}

#[derive(Debug)]
pub(crate) struct Popup {
    /// The node for which we are displaying the popup for
    node: Option<PeerId>,

    /// The content/component being diplayed
    pub content: Option<PopUpContent>,

    /// Component for `NodeCommmands` content
    pub node_commands: NodeCommands,

    /// Component for `NodeCommmands` content
    pub node_info: NodeInfo,

    /// Component for `IdentifyInfo` content
    pub identify_info: IdentifyInfo,

    /// Component for `KademliaInfo` content
    pub kademlia_info: KademliaInfo,

    /// Component for `NodeCommmands` content
    pub manage_connections: ManageConnections,

    /// Component for `KvStore` content
    pub kv_store: KvStore,
}

impl Popup {
    pub fn new() -> Self {
        Self {
            node: None,
            content: None,
            node_commands: NodeCommands::new(),
            node_info: NodeInfo::new(),
            identify_info: IdentifyInfo::new(),
            kademlia_info: KademliaInfo::new(),
            manage_connections: ManageConnections::new(),
            kv_store: KvStore::new(),
        }
    }

    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
        self.node_commands.set_node(node);
        self.manage_connections.set_node(node);
        self.node_info.set_node(node);
        self.identify_info.set_node(node);
        self.kademlia_info.set_node(node);
        self.kv_store.set_node(node);
    }

    pub fn set_content(&mut self, content: PopUpContent) {
        self.content = Some(content);
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
        node_connections: &HashMap<PeerId, HashSet<PeerId>>,
        show_external: bool,
    ) {
        match self.content {
            Some(PopUpContent::NodeInfo) => {
                self.node_info.render(frame, area);
            }
            Some(PopUpContent::NodeCommands) => {
                self.node_commands.render(frame, area);
            }
            Some(PopUpContent::ManageConnections) => {
                self.manage_connections.render(
                    frame,
                    area,
                    active_nodes,
                    external_nodes,
                    node_connections,
                    show_external,
                );
            }
            Some(PopUpContent::IdentifyInfo) => {
                self.identify_info.render(frame, area);
            }
            Some(PopUpContent::KademliaInfo) => {
                self.kademlia_info.render(frame, area);
            }
            Some(PopUpContent::KvStore) => {
                self.kv_store.render(frame, area);
            }
            _ => {}
        }
    }

    pub fn update(
        &mut self,
        action: &Action,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
    ) {
        match action {
            Action::DisplayManageConnections { .. } => {
                self.set_content(PopUpContent::ManageConnections);
            }
            Action::DisplayInfo { .. } => {
                self.set_content(PopUpContent::NodeInfo);
            }
            Action::DisplayIdentifyInfo { .. } => {
                self.set_content(PopUpContent::IdentifyInfo);
            }
            Action::DisplayKademliaInfo { .. } => {
                self.set_content(PopUpContent::KademliaInfo);
            }
            Action::DisplayKvStore { .. } => {
                self.set_content(PopUpContent::KvStore);
            }
            _ => {}
        }

        self.node_commands.update(action, actions);
        self.node_info.update(action, actions);
        self.manage_connections
            .update(action, actions, active_nodes);
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut ActionQueue,
        active_nodes: &IndexSet<PeerId>,
        external_nodes: &IndexSet<PeerId>,
        show_external: bool,
    ) -> Result<()> {
        match self.content {
            Some(PopUpContent::NodeCommands) => {
                self.node_commands.handle_key_event(key_event, actions)?;
            }
            Some(PopUpContent::ManageConnections) => self.manage_connections.handle_key_event(
                key_event,
                actions,
                active_nodes,
                external_nodes,
                show_external,
            )?,
            Some(PopUpContent::NodeInfo) => self.node_info.handle_key_event(key_event, actions)?,
            Some(PopUpContent::KvStore) => self.kv_store.handle_key_event(key_event, actions),
            Some(PopUpContent::IdentifyInfo) => {
                self.identify_info.handle_key_event(key_event, actions)?;
            }
            Some(PopUpContent::KademliaInfo) => {
                self.kademlia_info.handle_key_event(key_event, actions)?;
            }
            _ => {}
        }

        Ok(())
    }
}

impl TuiEventHandler for Popup {
    fn on_network_event(
        &mut self,
        event: &crate::messages::NetworkEvent,
        actions: &mut ActionQueue,
    ) {
        // Forward to all child components; each one self-filters.
        self.identify_info.on_network_event(event, actions);
        self.kademlia_info.on_network_event(event, actions);
    }

    fn on_key(
        &mut self,
        key: KeyEvent,
        actions: &mut ActionQueue,
        ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(
            key,
            actions,
            ctx.active_nodes,
            ctx.external_nodes,
            ctx.transport_tcp,
        ) {
            warn!(target: "tui::popup", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(&mut self, action: &Action, actions: &mut ActionQueue, active: &IndexSet<PeerId>) {
        self.update(action, actions, active);
    }
}
