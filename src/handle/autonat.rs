use crate::handle::Connexa;
use crate::types::AutonatCommand;
use futures::channel::oneshot;
use libp2p::autonat::NatStatus;
use libp2p::{Multiaddr, PeerId};
use std::io;

#[derive(Copy, Clone)]
pub struct ConnexaAutonat<'a, T = ()> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaAutonat<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Returns the assumed public address of the local node.
    pub async fn public_address(&self) -> io::Result<Option<Multiaddr>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(AutonatCommand::PublicAddress { resp: tx }.into())
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    /// Returns the current NAT status of the local node
    pub async fn nat_status(&self) -> io::Result<NatStatus> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(AutonatCommand::NatStatus { resp: tx }.into())
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    /// Adds a peer to the list of servers used for probes.
    pub async fn add_server(&self, peer_id: PeerId, address: Option<Multiaddr>) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                AutonatCommand::AddServer {
                    peer: peer_id,
                    address,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    /// Remove a peer from the list of servers.
    pub async fn remove_server(&self, peer_id: PeerId) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                AutonatCommand::RemoveServer {
                    peer: peer_id,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    /// Probes a specific address for external reachability
    pub async fn probe(&self, address: Multiaddr) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(AutonatCommand::Probe { address, resp: tx }.into())
            .await?;
        rx.await.map_err(io::Error::other)?
    }
}
