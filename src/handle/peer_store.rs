use crate::handle::Connexa;
use crate::prelude::PeerId;
use crate::types::PeerstoreCommand;
use futures::channel::oneshot;
use libp2p::Multiaddr;

#[derive(Copy, Clone)]
pub struct ConnexaPeerstore<'a, T = ()> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaPeerstore<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    pub async fn add_address(&self, peer_id: PeerId, addr: Multiaddr) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                PeerstoreCommand::Add {
                    peer_id,
                    addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)??.await
    }

    pub async fn remove_address(&self, peer_id: PeerId, addr: Multiaddr) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                PeerstoreCommand::RemoveAddress {
                    peer_id,
                    addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)??.await
    }

    pub async fn remove_peer(&self, peer_id: PeerId) -> std::io::Result<Vec<Multiaddr>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(PeerstoreCommand::Remove { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)??.await
    }

    pub async fn list(&self, peer_id: PeerId) -> std::io::Result<Vec<Multiaddr>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(PeerstoreCommand::List { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)??.await
    }
}
