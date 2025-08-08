use crate::behaviour::peer_store::store::Store;
use crate::prelude::swarm::FromSwarm;
use crate::prelude::{Multiaddr, PeerId};
use either::Either;
use std::task::{Context, Poll};

impl<L: Store, R: Store> Store for Either<L, R> {
    type Event = Either<L::Event, R::Event>;

    fn insert(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static {
        match self {
            Either::Left(l) => l.insert(peer_id, address),
            Either::Right(r) => r.insert(peer_id, address),
        }
    }

    fn remove(
        &mut self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static {
        match self {
            Either::Left(l) => l.remove(peer_id),
            Either::Right(r) => r.remove(peer_id),
        }
    }

    fn remove_address(
        &mut self,
        peer_id: &PeerId,
        address: &Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static {
        match self {
            Either::Left(l) => l.remove_address(peer_id, address),
            Either::Right(r) => r.remove_address(peer_id, address),
        }
    }

    fn address(
        &self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static {
        match self {
            Either::Left(l) => l.address(peer_id),
            Either::Right(r) => r.address(peer_id),
        }
    }

    fn in_memory_address(&self, peer_id: &PeerId) -> Vec<Multiaddr> {
        match self {
            Either::Left(l) => l.in_memory_address(peer_id),
            Either::Right(r) => r.in_memory_address(peer_id),
        }
    }

    fn on_swarm_event(&mut self, event: &FromSwarm) {
        match self {
            Either::Left(l) => l.on_swarm_event(event),
            Either::Right(r) => r.on_swarm_event(event),
        }
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Event> {
        match self {
            Either::Left(l) => l.poll(cx),
            Either::Right(r) => r.poll(cx),
        }
    }
}
