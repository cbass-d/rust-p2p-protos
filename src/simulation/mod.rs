use tokio::{sync::mpsc, task::JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    error::AppError,
    messages::{NetworkCommand, NetworkEvent},
    network::{self, NodeNetwork},
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

        // Build the channels to be used
        let (network_event_tx, network_event_rx) =
            mpsc::channel::<NetworkEvent>(EVENT_CHANNEL_SIZE);
        let (network_command_tx, network_command_rx) =
            mpsc::channel::<NetworkCommand>(EVENT_CHANNEL_SIZE);

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

    pub async fn run(self) -> Result<(), AppError> {
        // The task set will hold the TUI taks and the node network tasks
        // Using task set makes it easier to manage the waiting on tasks to finish
        let mut task_set: JoinSet<Result<(), AppError>> = JoinSet::new();

        // Run the TUI task
        let mut app = self.app;
        let app_task = task_set.spawn(async move { app.run().await });

        // Run the node network task
        let mut node_network = self.node_network;
        let network_task = task_set.spawn(async move { node_network.run().await });

        // Wait for all the tasks to finish gracefully
        while let Some(task_result) = task_set.join_next_with_id().await {
            match task_result {
                Ok(result) if result.0 == app_task.id() => {
                    info!("TUI app task finished: {result:#?}");
                }
                Ok(result) if result.0 == network_task.id() => {
                    info!("Node network task finished: {result:#?}");
                }
                Err(e) if e.id() == app_task.id() => {
                    error!("Node network task failed to complete: {e}");
                }
                Err(e) if e.id() == network_task.id() => {
                    error!("TUI app task failed to complete: {e}");
                }
                _ => {}
            }
        }

        Ok(())
    }
}
