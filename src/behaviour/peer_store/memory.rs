use crate::behaviour::peer_store::store::Store;
use crate::prelude::swarm::derive_prelude::ConnectionEstablished;
use crate::prelude::swarm::{ConnectionClosed, FromSwarm, NewExternalAddrOfPeer};
use futures::future::{Ready, ready};
use futures::{FutureExt, StreamExt};
use futures_timer::Delay;
use indexmap::map::Entry;
use indexmap::{IndexMap, IndexSet};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use pollable_map::futures::FutureMap;
use std::task::{Context, Poll};

#[derive(Default)]
pub struct MemoryStore {
    peers: IndexMap<PeerId, IndexSet<Multiaddr>>,
    // Note: we do this to act as a "reference counter" to the same address connected to the peer
    //       before we proceed to mark the address for removal.
    connections: IndexMap<PeerId, IndexMap<Multiaddr, IndexSet<ConnectionId>>>,
    persistent: IndexSet<PeerId>,
    timer: FutureMap<(PeerId, Multiaddr), Delay>,
}

// Note that we use this because the trait returns a future, which would allow the implementation to either use `async` to desugar to `fn -> impl Future`
// or allow custom futures (ie here we use `Ready` since this is in-memory and is expected to be ready). This, however, may change in the future.
// See https://github.com/rust-lang/rust/issues/121718 for more information
#[allow(refining_impl_trait)]
impl Store for MemoryStore {
    type Event = ();

    fn insert(&mut self, peer_id: PeerId, address: Multiaddr) -> Ready<std::io::Result<()>> {
        let result = match self.peers.entry(peer_id).or_default().insert(address) {
            true => {
                self.persistent.insert(peer_id);
                Ok(())
            }
            false => Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "address already exists",
            )),
        };
        ready(result)
    }

    fn remove(&mut self, peer_id: &PeerId) -> Ready<std::io::Result<Vec<Multiaddr>>> {
        let result = {
            let list = self.peers.shift_remove(peer_id);
            match list {
                Some(list) => {
                    self.persistent.shift_remove(peer_id);
                    Ok(Vec::from_iter(list))
                }
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "peer not found",
                )),
            }
        };
        ready(result)
    }

    fn remove_address(
        &mut self,
        peer_id: &PeerId,
        address: &Multiaddr,
    ) -> Ready<std::io::Result<()>> {
        if let Entry::Occupied(mut entry) = self.peers.entry(*peer_id) {
            if entry.get_mut().shift_remove(address) {
                return ready(Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "address not found",
                )));
            }
            if entry.get().is_empty() {
                entry.shift_remove();
                self.persistent.shift_remove(peer_id);
            }
            return ready(Ok(()));
        }
        ready(Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "peer not found",
        )))
    }

    fn address(&self, peer_id: &PeerId) -> Ready<std::io::Result<Vec<Multiaddr>>> {
        let Some(addrs) = self.peers.get(peer_id).cloned() else {
            return ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "peer not found",
            )));
        };
        ready(Ok(Vec::from_iter(addrs)))
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

                if let Entry::Occupied(mut entry) = self.connections.entry(*peer_id) {
                    let list = entry.get_mut();
                    if let Entry::Occupied(mut ma_entry) = list.entry(remote_addr.clone()) {
                        let connections = ma_entry.get_mut();
                        connections.shift_remove(connection_id);
                        if connections.is_empty() {
                            ma_entry.shift_remove();
                            self.timer.insert(
                                (*peer_id, remote_addr.clone()),
                                Delay::new(std::time::Duration::from_secs(60)),
                            );
                        }
                    }
                    if list.is_empty() {
                        entry.shift_remove();
                    }
                }
            }
            _ => {}
        }
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Event> {
        while let Poll::Ready(Some(((peer_id, addr), _))) = self.timer.poll_next_unpin(cx) {
            if let Err(e) = self
                .remove_address(&peer_id, &addr)
                .now_or_never()
                .expect("future ready")
            {
                tracing::error!(%peer_id, %addr, error = %e, "failed to remove address from store");
            }
        }
        Poll::Pending
    }
}
