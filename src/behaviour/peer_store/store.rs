pub mod dummy;
pub mod memory;

use libp2p::swarm::FromSwarm;
use libp2p::{Multiaddr, PeerId};
use std::fmt::Debug;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll};

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StoreId(usize);

impl StoreId {
    pub fn new_unchecked(id: usize) -> Self {
        Self(id)
    }

    pub fn next() -> Self {
        Self(ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }

    pub fn get(self) -> usize {
        self.0
    }
}

pub enum Event<T> {
    Inserted {
        id: StoreId,
    },
    Removed {
        id: StoreId,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    RemovedAddress {
        id: StoreId,
    },
    Get {
        id: StoreId,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    ListAll {
        id: StoreId,
        list: Vec<(PeerId, Vec<Multiaddr>)>,
    },
    Error {
        id: StoreId,
        error: std::io::Error,
    },
    Custom(T),
}

pub trait Store: Send + Sync + 'static {
    type Event: Debug + Send;

    /// Insert a new peer and associated address into the store
    fn insert(&mut self, peer_id: PeerId, address: Multiaddr) -> StoreId;

    /// Remove a peer and all its associated addresses from the store
    fn remove(&mut self, peer_id: &PeerId) -> StoreId;

    /// Remove a specific address associated with a peer from the store
    fn remove_address(&mut self, peer_id: &PeerId, address: &Multiaddr) -> StoreId;

    /// Get all addresses associated with a peer
    fn address(&mut self, peer_id: &PeerId) -> StoreId;

    /// Get all addresses in the peer store
    fn list_all(&mut self) -> StoreId;

    /// Get all in-memory address associated with a peer.
    /// Note that this function assumes that the addresses are stored in memory
    fn in_memory_address(&self, peer_id: &PeerId) -> Vec<Multiaddr>;

    /// Handle swarm events to update the store state
    fn on_swarm_event(&mut self, event: &FromSwarm);

    /// Poll the store for events
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Event<Self::Event>>;
}

/*
pub trait Store: Send + Sync + 'static {
    type Event: Debug + Send;

    /// Insert a new peer and associated address into the store
    fn insert(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static;

    /// Remove a peer and all its associated addresses from the store
    fn remove(
        &mut self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static;

    /// Remove a specific address associated with a peer from the store
    fn remove_address(
        &mut self,
        peer_id: &PeerId,
        address: &Multiaddr,
    ) -> impl Future<Output = std::io::Result<()>> + Send + 'static;

    /// Get all addresses associated with a peer
    fn address(
        &self,
        peer_id: &PeerId,
    ) -> impl Future<Output = std::io::Result<Vec<Multiaddr>>> + Send + 'static;

    /// Get all addresses in the peer store
    fn list_all(
        &self,
    ) -> impl Future<Output = std::io::Result<Vec<(PeerId, Vec<Multiaddr>)>>> + Send + 'static;

    /// Get all in-memory address associated with a peer.
    /// Note that this function assumes that the addresses are stored in memory
    fn in_memory_address(&self, peer_id: &PeerId) -> Vec<Multiaddr>;

    /// Handle swarm events to update the store state
    fn on_swarm_event(&mut self, event: &FromSwarm);

    /// Poll the store for events
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Event>;
}

 */
