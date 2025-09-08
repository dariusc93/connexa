use crate::handle::Connexa;
use crate::prelude::{Multiaddr, PeerId};
use crate::types::AutoRelayCommand;
use futures::channel::oneshot;
use std::io;

pub struct ConnexaRelay<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaRelay<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    pub async fn add_static_relay(&self, peer_id: PeerId, addr: Multiaddr) -> io::Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                AutoRelayCommand::AddStaticRelay {
                    peer_id,
                    relay_addr: addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    pub async fn remove_static_relay(&self, peer_id: PeerId, addr: Multiaddr) -> io::Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                AutoRelayCommand::RemoveStaticRelay {
                    peer_id,
                    relay_addr: addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    pub async fn enable_auto_relay(&self) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(AutoRelayCommand::EnableAutoRelay { resp: tx }.into())
            .await?;
        rx.await.map_err(io::Error::other)?
    }

    pub async fn disable_auto_relay(&self) -> io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(AutoRelayCommand::DisableAutoRelay { resp: tx }.into())
            .await?;
        rx.await.map_err(io::Error::other)?
    }
}
