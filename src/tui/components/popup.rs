use std::collections::VecDeque;

use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use libp2p::PeerId;
use ratatui::{Frame, layout::Rect};

use crate::tui::{
    app::Action,
    components::{
        identify_info::IdentifyInfo, kademlia_info::KademliaInfo,
        manage_connections::ManageConnections, node_commands::NodeCommands, node_info::NodeInfo,
    },
};

/// What content/component should the Popup display
#[derive(Debug, Clone, Copy)]
pub enum PopUpContent {
    NodeCommands,
    ManageConnections,
    NodeInfo,
    IdentifyInfo,
    KademliaInfo,
}

#[derive(Debug)]
pub struct Popup {
    /// The node for which we are displaying the popup for
    node: Option<PeerId>,

    /// The content/component being diplayed
    pub content: Option<PopUpContent>,

    /// If the component is currenlty in focus in the TUI
    focus: bool,

    /// Component for NodeCommmands content
    pub node_commands: NodeCommands,

    /// Component for NodeCommmands content
    pub node_info: NodeInfo,

    /// Component for IdentifyInfo content
    pub identify_info: IdentifyInfo,

    /// Component for KademliaInfo content
    pub kademlia_info: KademliaInfo,

    /// Component for NodeCommmands content
    pub manage_connections: ManageConnections,
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
            focus: false,
        }
    }

    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
        self.node_commands.set_node(node);
        self.manage_connections.set_node(node);
        self.node_info.set_node(node);
        self.identify_info.set_node(node);
        self.kademlia_info.set_node(node);
    }

    pub fn set_content(&mut self, content: PopUpContent) {
        self.content = Some(content);
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        match self.content {
            Some(PopUpContent::NodeInfo) => {
                self.node_info.render(frame, area);
            }
            Some(PopUpContent::NodeCommands) => {
                self.node_commands.render(frame, area);
            }
            Some(PopUpContent::ManageConnections) => {
                self.manage_connections.render(frame, area);
            }
            Some(PopUpContent::IdentifyInfo) => {
                self.identify_info.render(frame, area);
            }
            Some(PopUpContent::KademliaInfo) => {
                self.kademlia_info.render(frame, area);
            }
            _ => {}
        }
    }

    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
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
            _ => {}
        }

        self.node_commands.update(action.clone(), actions);
        self.node_info.update(action.clone(), actions);
        self.manage_connections.update(action.clone(), actions);
    }

    pub async fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match self.content {
            Some(PopUpContent::NodeCommands) => {
                self.node_commands.handle_key_event(key_event, actions)?
            }
            Some(PopUpContent::ManageConnections) => self
                .manage_connections
                .handle_key_event(key_event, actions)?,
            Some(PopUpContent::NodeInfo) => self.node_info.handle_key_event(key_event, actions)?,
            Some(PopUpContent::IdentifyInfo) => {
                self.identify_info.handle_key_event(key_event, actions)?
            }
            Some(PopUpContent::KademliaInfo) => {
                self.kademlia_info.handle_key_event(key_event, actions)?
            }
            _ => {}
        }

        Ok(())
    }
}
