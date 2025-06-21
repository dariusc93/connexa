use crate::handle::Connexa;
use crate::types::{ConnectionEvent, SwarmCommand};
use either::Either;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::core::transport::ListenerId;
use libp2p::swarm::ConnectionId;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId};

#[derive(Copy, Clone)]
pub struct ConnexaSwarm<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaSwarm<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Initiates a dial attempt to connect to a remote peer using provided dial options
    /// Returns the ConnectionId upon successful connection
    pub async fn dial(&self, target: impl Into<DialOpts>) -> crate::handle::Result<ConnectionId> {
        let target = target.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                SwarmCommand::Dial {
                    opt: target,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Disconnects from a peer specified by either PeerId or ConnectionId
    pub async fn disconnect(
        &self,
        target_type: impl Into<Either<PeerId, ConnectionId>>,
    ) -> crate::handle::Result<()> {
        let target_type = target_type.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                SwarmCommand::Disconnect {
                    target_type,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Checks if we are connected to a specific peer
    pub async fn is_connected(&self, peer_id: PeerId) -> crate::handle::Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::IsConnected { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)
    }

    /// Returns a list of all currently connected peers
    pub async fn connected_peers(&self) -> crate::handle::Result<Vec<PeerId>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::ConnectedPeers { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)
    }

    /// Start listening for incoming connections on the given multiaddress
    pub async fn listen_on(&self, address: Multiaddr) -> crate::handle::Result<ListenerId> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::ListenOn { address, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Stops listening on the address associated with the given ListernerId
    pub async fn remove_listener(&self, listener_id: ListenerId) -> crate::handle::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                SwarmCommand::RemoveListener {
                    listener_id,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Adds an external address that other peers can use to reach us
    pub async fn add_external_address(&self, address: Multiaddr) -> crate::handle::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::AddExternalAddress { address, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Removes an external address
    pub async fn remove_external_address(&self, address: Multiaddr) -> crate::handle::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::RemoveExternalAddress { address, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Returns a list of all external addresses that other peers can use to reach us
    pub async fn external_addresses(&self) -> crate::handle::Result<Vec<Multiaddr>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::ListExternalAddresses { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)
    }

    /// Returns a list of all addresses we are currently listening on
    pub async fn listening_addresses(&self) -> crate::handle::Result<Vec<Multiaddr>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::ListListeningAddresses { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)
    }

    /// Associates an address with a PeerId in the local address book
    pub async fn add_peer_address(
        &self,
        peer_id: PeerId,
        address: Multiaddr,
    ) -> crate::handle::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                SwarmCommand::AddPeerAddress {
                    peer_id,
                    address,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Subscribes to swarm connection events.
    pub async fn listener(&self) -> std::io::Result<BoxStream<'static, ConnectionEvent>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(SwarmCommand::Listener { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other).map(|rx| rx.boxed())
    }
}
