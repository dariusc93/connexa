use crate::handle::Connexa;
use crate::prelude::PeerId;
use crate::types::BlacklistCommand;
use futures::channel::oneshot;

#[derive(Copy, Clone)]
pub struct ConnexaBlacklist<'a, T = ()> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaBlacklist<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Adds a peer to the blacklist.
    pub async fn add(&self, peer_id: PeerId) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(BlacklistCommand::Add { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Removes a peer from the blacklist.
    pub async fn remove(&self, peer_id: PeerId) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(BlacklistCommand::Remove { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Retrieves the list of blacklisted peers.
    pub async fn list(&self) -> std::io::Result<Vec<PeerId>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(BlacklistCommand::List { resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }
}
