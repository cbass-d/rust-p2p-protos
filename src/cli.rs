use clap::Parser;

/// CLI arguments for the application
#[derive(Parser, Debug)]
pub struct CliArgs {
    /// The number of nodes the node network should
    /// start with
    #[arg(short, long, default_value_t = 5)]
    pub nodes: u8,
}
