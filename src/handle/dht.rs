use crate::handle::Connexa;
use crate::types::{DHTCommand, DHTEvent};
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

    pub async fn provide(&self, key: impl ToRecordKey) -> std::io::Result<()> {
        let key = key.to_record_key();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Provide { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn stop_provide(&self, key: impl ToRecordKey) -> std::io::Result<()> {
        let key = key.to_record_key();
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
        key: impl ToRecordKey,
    ) -> std::io::Result<BoxStream<'static, std::io::Result<HashSet<PeerId>>>> {
        let key = key.to_record_key();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::GetProviders { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?.map(|s| s.boxed())
    }

    pub async fn listener(
        &self,
        key: impl ToOptionalRecordKey,
    ) -> std::io::Result<BoxStream<'static, DHTEvent>> {
        let key = key.to_record_key();
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Listener { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?.map(|s| s.boxed())
    }

    pub async fn get(
        &self,
        key: impl ToRecordKey,
    ) -> std::io::Result<BoxStream<'static, std::io::Result<PeerRecord>>> {
        let key = key.to_record_key();
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
        key: impl ToRecordKey,
        data: impl Into<Bytes>,
        quorum: Quorum,
    ) -> std::io::Result<()> {
        let key = key.to_record_key();
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

pub trait ToRecordKey {
    fn to_record_key(self) -> RecordKey;
}

pub trait ToOptionalRecordKey {
    fn to_record_key(self) -> Option<RecordKey>;
}

impl ToRecordKey for RecordKey {
    fn to_record_key(self) -> RecordKey {
        self
    }
}

impl ToRecordKey for String {
    fn to_record_key(self) -> RecordKey {
        self.as_bytes().to_vec().into()
    }
}

impl ToRecordKey for &str {
    fn to_record_key(self) -> RecordKey {
        self.as_bytes().to_vec().into()
    }
}

impl ToRecordKey for Vec<u8> {
    fn to_record_key(self) -> RecordKey {
        self.into()
    }
}

impl ToRecordKey for &[u8] {
    fn to_record_key(self) -> RecordKey {
        self.to_vec().into()
    }
}

impl ToRecordKey for Bytes {
    fn to_record_key(self) -> RecordKey {
        // TODO: Implement conversion upstream
        self.to_vec().into()
    }
}

impl<R: ToRecordKey> ToOptionalRecordKey for R {
    fn to_record_key(self) -> Option<RecordKey> {
        Some(self.to_record_key())
    }
}

impl<R: ToRecordKey> ToOptionalRecordKey for Option<R> {
    fn to_record_key(self) -> Option<RecordKey> {
        self.map(|r| r.to_record_key())
    }
}
