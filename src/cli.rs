use std::net::Ipv4Addr;

use clap::Parser;

/// CLI arguments for the application
#[derive(Parser, Debug)]
pub(crate) struct CliArgs {
    /// The number of nodes the node network should start with
    #[arg(short, long, default_value_t = 5)]
    pub(crate) nodes: u8,

    /// The transport the nodes will operate on
    /// 'memory' or 'tcp'
    pub(crate) transport_mode: String,

    /// Bind address for tcp
    pub(crate) bind_address: Option<Ipv4Addr>,
}
