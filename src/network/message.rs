use std::net::Ipv4Addr;

pub struct Message {
    source: Ipv4Addr,
    destination: Ipv4Addr,
    data: Vec<u8>,
}

impl Message {
    pub fn new(source: Ipv4Addr, destination: Ipv4Addr, data: &[u8]) -> Self {
        Message {
            source,
            destination,
            data: data.to_owned(),
        }
    }

    pub fn source(&self) -> Ipv4Addr {
        self.source
    }

    pub fn destination(&self) -> Ipv4Addr {
        self.destination
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
