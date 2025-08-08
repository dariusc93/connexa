pub mod memory;

use libp2p::swarm::FromSwarm;
use libp2p::{Multiaddr, PeerId};
use std::fmt::Debug;
use std::task::{Context, Poll};

pub trait Store: Send + Sync + 'static {
    type Event: Debug + Send;
    fn insert(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static;
    fn remove(
        &mut self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static;
    fn remove_address(
        &mut self,
        peer_id: &PeerId,
        address: &Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static;
    fn address(
        &self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static;

    fn in_memory_address(&self, peer_id: &PeerId) -> Vec<Multiaddr>;

    fn on_swarm_event(&mut self, event: &FromSwarm);

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Event>;
}
