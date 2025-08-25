use libp2p::Multiaddr;
use libp2p::multiaddr::Protocol;

pub(crate) trait MultiaddrExt {
    fn is_relayed(&self) -> bool;
}

impl MultiaddrExt for Multiaddr {
    fn is_relayed(&self) -> bool {
        self.iter().any(|protocol| protocol == Protocol::P2pCircuit)
    }
}
