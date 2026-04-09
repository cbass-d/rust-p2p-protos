use std::net::Ipv4Addr;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, instrument};

use crate::{
    error::AppError,
    messages::{NetworkCommand, NetworkEvent},
    network::{NodeNetwork, TransportMode},
    tui::app::App,
};

const EVENT_CHANNEL_SIZE: usize = 10;
const COMMAND_CHANNEL_SIZE: usize = 10;

#[derive(Debug, Default)]
pub(crate) struct SimulationBuilder {
    max_nodes: u8,
    starting_nodes: u8,
    tick_rate: f64,
    frame_rate: f64,
    transport: TransportMode,
    bind_address: Option<Ipv4Addr>,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_nodes(mut self, max_nodes: u8) -> SimulationBuilder {
        self.max_nodes = max_nodes;
        self
    }

    pub fn starting_nodes(mut self, nodes: u8) -> SimulationBuilder {
        self.starting_nodes = nodes;
        self
    }

    pub fn tick_rate(mut self, tick_rate: f64) -> SimulationBuilder {
        self.tick_rate = tick_rate;
        self
    }

    pub fn frame_rate(mut self, frame_rate: f64) -> SimulationBuilder {
        self.frame_rate = frame_rate;
        self
    }

    pub fn transport(mut self, transport: TransportMode) -> SimulationBuilder {
        self.transport = transport;
        self
    }

    pub fn bind_address(mut self, bind_address: Option<Ipv4Addr>) -> SimulationBuilder {
        self.bind_address = bind_address;
        self
    }

    pub fn build(self) -> Result<Simulation, AppError> {
        // Do some checks for the passed values
        if self.starting_nodes > self.max_nodes {
            return Err(AppError::BuildError(format!(
                "starting nodes {0} > than max nodes {1}",
                self.starting_nodes, self.max_nodes
            )));
        }

        if self.tick_rate < 1.0 || self.tick_rate > 60.0 {
            return Err(AppError::BuildError(format!(
                "invalid tick rate {0}: must be between 1.0 and 60.0",
                self.tick_rate
            )));
        }

        if self.frame_rate < 1.0 || self.frame_rate > 60.0 {
            return Err(AppError::BuildError(format!(
                "invalid frame rate {0}: must be between 1.0 and 60.0",
                self.frame_rate
            )));
        }

        let (network_event_tx, network_event_rx) =
            mpsc::channel::<NetworkEvent>(EVENT_CHANNEL_SIZE);
        let (network_command_tx, network_command_rx) =
            mpsc::channel::<NetworkCommand>(COMMAND_CHANNEL_SIZE);

        let cancellation_token = CancellationToken::new();

        let app = App::new(
            cancellation_token.clone(),
            network_event_rx,
            network_command_tx,
            self.tick_rate,
            self.frame_rate,
        );

        let node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token.clone(),
            self.max_nodes,
            self.starting_nodes,
            self.transport,
            self.bind_address,
        );

        Ok(Simulation {
            app,
            node_network,
            cancellation_token,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Simulation {
    app: App,
    node_network: NodeNetwork,
    cancellation_token: CancellationToken,
}

impl Simulation {
    pub fn builder() -> SimulationBuilder {
        SimulationBuilder::new()
    }

    #[instrument(skip_all)]
    pub async fn run(self) -> Result<(), AppError> {
        let mut app = self.app;
        let mut node_network = self.node_network;

        let result = tokio::select! {
            result = app.run() => result,
            result = node_network.run() => {
                match &result {
                    Ok(()) => debug!(target: "simulation", "node network has finished"),
                    Err(e) => error!(target: "simulation", "node network exited with an error: {e}"),
                }
                self.cancellation_token.cancel();
                result.map_err(AppError::from)
            }
        };

        ratatui::restore();

        result
    }
}
