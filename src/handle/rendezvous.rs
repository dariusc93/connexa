use crate::handle::Connexa;
use crate::types::RendezvousCommand;
use futures::channel::oneshot;
use libp2p::rendezvous::Cookie;
use libp2p::{Multiaddr, PeerId};
use std::io;

#[derive(Copy, Clone)]
pub struct ConnexaRendezvous<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaRendezvous<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Registers a peer in a namespace with an optional time-to-live (TTL).
    pub async fn register(
        &self,
        peer_id: PeerId,
        namespace: impl Into<String>,
        ttl: Option<u64>,
    ) -> io::Result<()> {
        let namespace = namespace.into();

        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                RendezvousCommand::Register {
                    namespace,
                    peer_id,
                    ttl,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(io::Error::other)?
    }

    /// Unregisters a peer from a namespace.
    pub async fn unregister(
        &self,
        peer_id: PeerId,
        namespace: impl Into<String>,
    ) -> io::Result<()> {
        let namespace = namespace.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                RendezvousCommand::Unregister {
                    namespace,
                    peer_id,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(io::Error::other)?
    }

    /// Discovers peers in a namespace.
    pub async fn discovery(
        &self,
        peer_id: PeerId,
        namespace: impl Into<Option<String>>,
        ttl: Option<u64>,
        cookie: Option<Cookie>,
    ) -> io::Result<(Cookie, Vec<(PeerId, Vec<Multiaddr>)>)> {
        let namespace = namespace.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                RendezvousCommand::Discover {
                    namespace,
                    peer_id,
                    cookie,
                    ttl,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(io::Error::other)?
    }
}
