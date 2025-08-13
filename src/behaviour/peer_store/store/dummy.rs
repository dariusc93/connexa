use crate::behaviour::peer_store::store::Store;
use crate::prelude::peer_store::store::StoreId;
use crate::prelude::swarm::FromSwarm;
use crate::prelude::{Multiaddr, PeerId};
use std::task::{Context, Poll};

use super::Event;

#[derive(Debug, Copy, Clone)]
pub struct Dummy;

impl Store for Dummy {
    type Event = ();

    fn insert(&mut self, _: PeerId, _: Multiaddr) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn remove(&mut self, _: &PeerId) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn remove_address(&mut self, _: &PeerId, _: &Multiaddr) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn address(&mut self, _: &PeerId) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn list_all(&mut self) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn in_memory_address(&self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn on_swarm_event(&mut self, _: &FromSwarm) {}

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<Event<Self::Event>> {
        Poll::Pending
    }
}

impl Store for () {
    type Event = ();

    fn insert(&mut self, _: PeerId, _: Multiaddr) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn remove(&mut self, _: &PeerId) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn remove_address(&mut self, _: &PeerId, _: &Multiaddr) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn address(&mut self, _: &PeerId) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn list_all(&mut self) -> StoreId {
        StoreId::new_unchecked(0)
    }

    fn in_memory_address(&self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn on_swarm_event(&mut self, _: &FromSwarm) {}

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<Event<Self::Event>> {
        Poll::Pending
    }
}
