use network::NodeNetwork;
use std::net::Ipv4Addr;

mod network;
mod node;

fn main() {
    let mut network = NodeNetwork::new("10.0.0.1".parse().unwrap(), "10.0.0.254".parse().unwrap());

    for _ in 1..5 {
        let node = network.add_node().unwrap();

        println!("{node}");
    }
}
