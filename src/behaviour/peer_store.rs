pub mod store;

use crate::behaviour::peer_store::store::memory::MemoryStore;
use crate::behaviour::peer_store::store::{Event, Store};
use crate::prelude::peer_store::store::StoreId;
use crate::prelude::swarm::derive_prelude::PortUse;
use crate::prelude::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use crate::prelude::transport::Endpoint;
use crate::prelude::{Multiaddr, PeerId};
use futures::FutureExt;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::task::{Context, Poll, Waker};

pub struct Behaviour<S = MemoryStore>
where
    S: 'static,
{
    store: S,
    pending_response: HashMap<StoreId, EventResponse>,
    waker: Option<Waker>,
}

impl Default for Behaviour<MemoryStore> {
    fn default() -> Self {
        Self {
            waker: None,
            store: MemoryStore::default(),
            pending_response: Default::default(),
        }
    }
}

enum EventResponse {
    Inserted {
        resp: oneshot::Sender<std::io::Result<()>>,
    },
    Removed {
        resp: oneshot::Sender<std::io::Result<Vec<Multiaddr>>>,
    },
    RemovedAddress {
        resp: oneshot::Sender<std::io::Result<()>>,
    },
    Get {
        resp: oneshot::Sender<std::io::Result<Vec<Multiaddr>>>,
    },
    ListAll {
        resp: oneshot::Sender<std::io::Result<Vec<(PeerId, Vec<Multiaddr>)>>>,
    },
}

impl<S> Behaviour<S>
where
    S: Store,
{
    pub fn new(store: S) -> Self {
        Self {
            waker: None,
            store,
            pending_response: Default::default(),
        }
    }

    pub fn insert(
        &mut self,
        peer_id: PeerId,
        address: Multiaddr,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        let (tx, rx) = oneshot::channel();
        let id = self.store.insert(peer_id, address);
        self.pending_response
            .insert(id, EventResponse::Inserted { resp: tx });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        async move {
            match rx.await {
                Ok(res) => res,
                Err(c) => Err(std::io::Error::other(c)),
            }
        }
        .boxed()
    }

    pub fn remove(
        &mut self,
        peer_id: &PeerId,
    ) -> BoxFuture<'static, std::io::Result<Vec<Multiaddr>>> {
        let (tx, rx) = oneshot::channel();
        let id = self.store.remove(peer_id);
        self.pending_response
            .insert(id, EventResponse::Removed { resp: tx });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        async move {
            match rx.await {
                Ok(res) => res,
                Err(c) => Err(std::io::Error::other(c)),
            }
        }
        .boxed()
    }

    pub fn remove_address(
        &mut self,
        peer_id: &PeerId,
        address: &Multiaddr,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        let (tx, rx) = oneshot::channel();
        let id = self.store.remove_address(peer_id, address);
        self.pending_response
            .insert(id, EventResponse::RemovedAddress { resp: tx });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        async move {
            match rx.await {
                Ok(res) => res,
                Err(c) => Err(std::io::Error::other(c)),
            }
        }
        .boxed()
    }

    pub fn address(
        &mut self,
        peer_id: &PeerId,
    ) -> BoxFuture<'static, std::io::Result<Vec<Multiaddr>>> {
        let (tx, rx) = oneshot::channel();
        let id = self.store.address(peer_id);
        self.pending_response
            .insert(id, EventResponse::Get { resp: tx });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        async move {
            match rx.await {
                Ok(res) => res,
                Err(c) => Err(std::io::Error::other(c)),
            }
        }
        .boxed()
    }

    pub fn list_all(
        &mut self,
    ) -> BoxFuture<'static, std::io::Result<Vec<(PeerId, Vec<Multiaddr>)>>> {
        let (tx, rx) = oneshot::channel();
        let id = self.store.list_all();
        self.pending_response
            .insert(id, EventResponse::ListAll { resp: tx });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        async move {
            match rx.await {
                Ok(res) => res,
                Err(c) => Err(std::io::Error::other(c)),
            }
        }
        .boxed()
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    fn process_event(&mut self, event: Event<S::Event>) {
        match event {
            Event::Inserted { id } => {
                if let Some(EventResponse::Inserted { resp }) = self.pending_response.remove(&id) {
                    let _ = resp.send(Ok(()));
                }
            }
            Event::Removed {
                id,
                peer_id: _,
                addresses,
            } => {
                if let Some(EventResponse::Removed { resp }) = self.pending_response.remove(&id) {
                    let _ = resp.send(Ok(addresses));
                }
            }
            Event::RemovedAddress { id } => {
                if let Some(EventResponse::RemovedAddress { resp }) =
                    self.pending_response.remove(&id)
                {
                    let _ = resp.send(Ok(()));
                }
            }
            Event::Get {
                id,
                peer_id: _,
                addresses,
            } => {
                if let Some(EventResponse::Get { resp }) = self.pending_response.remove(&id) {
                    let _ = resp.send(Ok(addresses));
                }
            }
            Event::ListAll { id, list } => {
                if let Some(EventResponse::ListAll { resp }) = self.pending_response.remove(&id) {
                    let _ = resp.send(Ok(list));
                }
            }
            Event::Error { id, error } => {
                let resp = self.pending_response.remove(&id).expect("entry is valid");

                match resp {
                    EventResponse::Inserted { resp } => {
                        let _ = resp.send(Err(error));
                    }
                    EventResponse::Removed { resp } => {
                        let _ = resp.send(Err(error));
                    }
                    EventResponse::RemovedAddress { resp } => {
                        let _ = resp.send(Err(error));
                    }
                    EventResponse::Get { resp } => {
                        let _ = resp.send(Err(error));
                    }
                    EventResponse::ListAll { resp } => {
                        let _ = resp.send(Err(error));
                    }
                }
            }
            Event::Custom(_) => {}
        }
    }
}

impl<S> NetworkBehaviour for Behaviour<S>
where
    S: Store + 'static,
{
    type ConnectionHandler = super::dummy::DummyHandler;
    type ToSwarm = S::Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(super::dummy::DummyHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(super::dummy::DummyHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let Some(peer_id) = _maybe_peer else {
            return Ok(vec![]);
        };

        let addrs = self.store.in_memory_address(&peer_id);
        Ok(addrs)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.store.on_swarm_event(&event)
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        _: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        loop {
            match self.store.poll(cx) {
                Poll::Ready(ev) => match ev {
                    Event::Custom(custom) => return Poll::Ready(ToSwarm::GenerateEvent(custom)),
                    event => {
                        self.process_event(event);
                    }
                },
                Poll::Pending => {
                    self.waker = Some(cx.waker().clone());
                    break;
                }
            }
        }

        Poll::Pending
    }
}
