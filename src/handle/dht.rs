use crate::handle::Connexa;
use crate::types::DHTCommand;
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::kad::{Mode, PeerInfo, PeerRecord, Quorum, RecordKey};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashSet;

pub struct ConnexaDht<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaDht<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    pub async fn find_peer(&self, peer_id: PeerId) -> std::io::Result<Vec<PeerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::FindPeer { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn provide(&self, key: impl Into<RecordKey>) -> std::io::Result<()> {
        let key = key.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Provide { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn stop_provide(&self, key: impl Into<RecordKey>) -> std::io::Result<()> {
        let key = key.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Provide { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn get_providers(
        &self,
        key: impl Into<RecordKey>,
    ) -> std::io::Result<BoxStream<'static, std::io::Result<HashSet<PeerId>>>> {
        let key = key.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::GetProviders { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?.map(|s| s.boxed())
    }

    pub async fn get(
        &self,
        key: impl Into<RecordKey>,
    ) -> std::io::Result<BoxStream<'static, std::io::Result<PeerRecord>>> {
        let key = key.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Get { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?.map(|s| s.boxed())
    }

    pub async fn put(
        &self,
        key: impl Into<RecordKey>,
        data: impl Into<Bytes>,
        quorum: Quorum,
    ) -> std::io::Result<()> {
        let key = key.into();
        let data = data.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::Put {
                    key,
                    data,
                    quorum,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn set_mode(&self, mode: impl Into<Option<Mode>>) -> std::io::Result<()> {
        let mode = mode.into();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::SetDHTMode { mode, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn mode(&self) -> std::io::Result<Mode> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::DHTMode { resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn add_address(&self, peer_id: PeerId, addr: Multiaddr) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::AddAddress {
                    peer_id,
                    addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }
}
