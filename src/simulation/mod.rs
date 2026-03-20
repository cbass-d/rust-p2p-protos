use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, instrument};

use crate::{
    error::AppError,
    messages::{NetworkCommand, NetworkEvent},
    network::NodeNetwork,
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
        );

        Ok(Simulation { app, node_network })
    }
}

#[derive(Debug)]
pub(crate) struct Simulation {
    app: App,
    node_network: NodeNetwork,
}

impl Simulation {
    pub fn builder() -> SimulationBuilder {
        SimulationBuilder::new()
    }

    #[instrument(skip_all)]
    pub async fn run(self) -> Result<(), AppError> {
        let mut app = self.app;

        let mut node_network = self.node_network;

        let (app_result, network_result) = tokio::try_join!(app.run(), node_network.run())?;

        debug!(target: "simulation", "tui app task ended: {app_result:#?}");
        debug!(target: "simulation", "node network task ended: {network_result:#?}");

        Ok(())
    }
}
