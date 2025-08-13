use super::Event;
use crate::behaviour::peer_store::store::Store;
use crate::prelude::peer_store::store::StoreId;
use crate::prelude::swarm::derive_prelude::ConnectionEstablished;
use crate::prelude::swarm::{ConnectionClosed, FromSwarm, NewExternalAddrOfPeer};
use futures::StreamExt;
use futures_timer::Delay;
use indexmap::{IndexMap, IndexSet};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use pollable_map::futures::FutureMap;
use std::collections::VecDeque;
use std::task::{Context, Poll, Waker};

#[derive(Default)]
pub struct MemoryStore {
    peers: IndexMap<PeerId, IndexSet<Multiaddr>>,
    // Note: we do this to act as a "reference counter" to the same address connected to the peer
    //       before we proceed to mark the address for removal.
    connections: IndexMap<PeerId, IndexMap<Multiaddr, IndexSet<ConnectionId>>>,
    persistent: IndexSet<PeerId>,
    timer: FutureMap<(PeerId, Multiaddr), Delay>,
    events: VecDeque<Event<<Self as Store>::Event>>,
    waker: Option<Waker>,
}

impl FromIterator<(PeerId, Multiaddr)> for MemoryStore {
    fn from_iter<T: IntoIterator<Item = (PeerId, Multiaddr)>>(iter: T) -> Self {
        let mut store = Self::default();
        for (peer_id, addr) in iter {
            store.persistent.insert(peer_id);
            store.peers.entry(peer_id).or_default().insert(addr);
        }
        store
    }
}

// Note that we use this because the trait returns a future, which would allow the implementation to either use `async` to desugar to `fn -> impl Future`
// or allow custom futures (ie here we use `Ready` since this is in-memory and is expected to be ready). This, however, may change in the future.
// See https://github.com/rust-lang/rust/issues/121718 for more information
#[allow(refining_impl_trait)]
impl Store for MemoryStore {
    type Event = ();

    fn insert(&mut self, peer_id: PeerId, address: Multiaddr) -> StoreId {
        let id = StoreId::next();

        self.persistent.insert(peer_id);
        // remove cleanup timer since the address is manually stored.
        self.timer.remove(&(peer_id, address.clone()));

        let event = match self.peers.entry(peer_id).or_default().insert(address) {
            true => Event::Inserted { id },
            false => Event::Error {
                id,
                error: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "address already exists",
                ),
            },
        };
        self.events.push_back(event);
        id
    }

    fn remove(&mut self, peer_id: &PeerId) -> StoreId {
        let id = StoreId::next();

        let list = self.peers.shift_remove(peer_id);
        let event = match list {
            Some(list) => {
                self.persistent.shift_remove(peer_id);
                Event::Removed {
                    id,
                    peer_id: *peer_id,
                    addresses: Vec::from_iter(list),
                }
            }
            None => Event::Error {
                id,
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "peer not found"),
            },
        };

        self.events.push_back(event);

        id
    }

    fn remove_address(&mut self, peer_id: &PeerId, address: &Multiaddr) -> StoreId {
        let id = StoreId::next();
        let Some(list) = self.peers.get_mut(peer_id) else {
            self.events.push_back(Event::Error {
                id,
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "peer not found"),
            });
            return id;
        };

        if !list.shift_remove(address) {
            self.events.push_back(Event::Error {
                id,
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "address not found"),
            });
            return id;
        }

        if list.is_empty() {
            self.peers.shift_remove(peer_id);
            self.persistent.shift_remove(peer_id);
        }

        self.events.push_back(Event::RemovedAddress { id });

        id
    }

    fn address(&mut self, peer_id: &PeerId) -> StoreId {
        let id = StoreId::next();
        let Some(addrs) = self.peers.get(peer_id).cloned() else {
            self.events.push_back(Event::Error {
                id,
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "peer not found"),
            });
            return id;
        };
        let list = Vec::from_iter(addrs);

        self.events.push_back(Event::Get {
            id,
            peer_id: *peer_id,
            addresses: list,
        });
        id
    }

    fn list_all(&mut self) -> StoreId {
        let id = StoreId::next();
        let list = self
            .peers
            .iter()
            .map(|(peer_id, list)| {
                let list = Vec::from_iter(list.clone());
                (*peer_id, list)
            })
            .collect::<Vec<_>>();
        self.events.push_back(Event::ListAll { id, list });
        id
    }

    fn in_memory_address(&self, peer_id: &PeerId) -> Vec<Multiaddr> {
        self.peers
            .get(peer_id)
            .cloned()
            .map(Vec::from_iter)
            .unwrap_or_default()
    }

    fn on_swarm_event(&mut self, event: &FromSwarm) {
        match event {
            FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer { peer_id, addr }) => {
                self.peers
                    .entry(*peer_id)
                    .or_default()
                    .insert(Multiaddr::clone(addr));
                self.persistent.insert(*peer_id);
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                failed_addresses: _,
                ..
            }) => {
                // Note: because we are adding the addresses from an established connection, we will not be persisting the address unless
                //       the address is added manually.
                let remote_addr = endpoint.get_remote_address();
                self.connections
                    .entry(*peer_id)
                    .or_default()
                    .entry(remote_addr.clone())
                    .or_default()
                    .insert(*connection_id);
                self.peers
                    .entry(*peer_id)
                    .or_default()
                    .insert(remote_addr.clone());
                // TODO: determine if we should remove any failed addresses from the store to keep the entry up to date?
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                connection_id,
                peer_id,
                endpoint,
                ..
            }) => {
                let remote_addr = endpoint.get_remote_address();

                let Some(connections) = self.connections.get_mut(peer_id) else {
                    return;
                };

                let Some(list) = connections.get_mut(remote_addr) else {
                    return;
                };

                list.shift_remove(connection_id);

                if !list.is_empty() {
                    return;
                }

                connections.shift_remove(remote_addr);
                if !connections.is_empty() {
                    return;
                }
                self.connections.shift_remove(peer_id);
                if !self.persistent.contains(peer_id) {
                    self.timer.insert(
                        (*peer_id, remote_addr.clone()),
                        Delay::new(std::time::Duration::from_secs(60)),
                    );
                }
                self.connections.shrink_to_fit();
            }
            _ => {}
        }
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Event<Self::Event>> {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        while let Poll::Ready(Some(((peer_id, addr), _))) = self.timer.poll_next_unpin(cx) {
            if self.persistent.contains(&peer_id) {
                continue;
            }

            let Some(list) = self.peers.get_mut(&peer_id) else {
                continue;
            };

            list.shift_remove(&addr);

            if list.is_empty() {
                self.peers.shift_remove(&peer_id);
            }
        }

        self.waker = Some(cx.waker().clone());

        Poll::Pending
    }
}
