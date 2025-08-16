use crate::handle::Connexa;
use crate::types::{DHTCommand, DHTEvent};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::kad::{Mode, PeerInfo, PeerRecord, Quorum, RecordKey};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashSet;

#[derive(Copy, Clone)]
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

    /// Queries the DHT for information about a specific peer by its PeerID
    pub async fn find_peer(&self, peer_id: PeerId) -> std::io::Result<Vec<PeerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::FindPeer { peer_id, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Announces to the DHT that this peer can provide data for a given key
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

    /// Stop announcing that this peer can provide data for a given key
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

    /// Queries the DHT for peers that can provide data for a given key
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

    /// Bootstraps the DHT node.
    /// Note that this will continue to wait until bootstrapping completes
    pub async fn bootstrap(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::Bootstrap {
                    lazy: false,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Lazily bootstraps the DHT node.
    /// Note that this will handle bootstrapping in the background
    pub async fn bootstrap_lazy(&self) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::Bootstrap {
                    lazy: true,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Creates a listener for DHT events related to a specific key
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

    /// Retrieves data from the DHT for a given key
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

    /// Stores data in the DHT under a given key with a specified quorum
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

    /// Stores a record in the DHT targeting specific peers with a specified quorum.
    /// Note that this operation does not store a record in the record store and will
    /// require that the record exist before using this method.
    ///
    /// See [Behaviour::put_record_to](libp2p::kad::Behaviour::put_record_to) for more information
    pub async fn put_to(
        &self,
        target: impl ExactSizeIterator<Item = PeerId>,
        key: impl ToRecordKey,
        data: impl Into<Bytes>,
        quorum: Quorum,
    ) -> std::io::Result<()> {
        let key = key.to_record_key();
        let data = data.into();
        let target = target.collect::<Vec<_>>();

        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::PutTo {
                    target,
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

    /// Removes a record from the local record store
    pub async fn remove_record(&self, key: impl ToRecordKey) -> std::io::Result<()> {
        let key = key.to_record_key();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::Remove { key, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Sets the DHT mode (Client/Server)
    /// Mode can be None to automatically switch between client and server based on reachability.
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

    /// Gets the current DHT mode
    pub async fn mode(&self) -> std::io::Result<Mode> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::DHTMode { resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Adds an address to the routing table for a specific peer
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

    /// Removes an address from the routing table for a specific peer
    pub async fn remove_address(&self, peer_id: PeerId, addr: Multiaddr) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(
                DHTCommand::RemoveAddress {
                    peer_id,
                    addr,
                    resp: tx,
                }
                .into(),
            )
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }

    /// Removes a peer from the routing table
    pub async fn remove_peer(&self, peer_id: PeerId) -> std::io::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(DHTCommand::RemovePeer { peer_id, resp: tx }.into())
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

#[cfg(feature = "cid")]
impl ToRecordKey for cid::Cid {
    fn to_record_key(self) -> RecordKey {
        self.hash().to_bytes().into()
    }
}

impl ToRecordKey for RecordKey {
    fn to_record_key(self) -> RecordKey {
        self
    }
}

impl ToRecordKey for &RecordKey {
    fn to_record_key(self) -> RecordKey {
        self.clone()
    }
}

impl ToRecordKey for String {
    fn to_record_key(self) -> RecordKey {
        self.into_bytes().into()
    }
}

impl ToRecordKey for &String {
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

impl<const N: usize> ToRecordKey for [u8; N] {
    fn to_record_key(self) -> RecordKey {
        self.to_vec().into()
    }
}

impl ToRecordKey for Bytes {
    fn to_record_key(self) -> RecordKey {
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

impl ToOptionalRecordKey for () {
    fn to_record_key(self) -> Option<RecordKey> {
        None
    }
}
