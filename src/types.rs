#![allow(unused_imports)]
use bytes::Bytes;
use either::Either;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use indexmap::IndexSet;
use libp2p::core::ConnectedPoint;
#[cfg(feature = "gossipsub")]
use libp2p::gossipsub::{MessageAcceptance, MessageId};
#[cfg(feature = "kad")]
use libp2p::kad::{Mode, PeerInfo, PeerRecord, ProviderRecord, Quorum, Record, RecordKey};
#[cfg(feature = "rendezvous")]
use libp2p::rendezvous::Cookie;
#[cfg(feature = "request-response")]
use libp2p::request_response::InboundRequestId;
use libp2p::swarm::ConnectionId;
use libp2p::swarm::derive_prelude::ListenerId;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use libp2p_connection_limits::ConnectionLimits;
use std::collections::HashSet;

type Result<T> = std::io::Result<T>;

#[derive(Debug)]
pub enum Command<T = ()> {
    Swarm(SwarmCommand),
    #[cfg(feature = "gossipsub")]
    Gossipsub(GossipsubCommand),
    #[cfg(feature = "floodsub")]
    Floodsub(FloodsubCommand),
    #[cfg(feature = "kad")]
    Dht(DHTCommand),
    #[cfg(feature = "request-response")]
    RequestResponse(RequestResponseCommand),
    #[cfg(feature = "stream")]
    Stream(StreamCommand),
    #[cfg(feature = "rendezvous")]
    Rendezvous(RendezvousCommand),
    #[cfg(feature = "autonat")]
    Autonat(AutonatCommand),
    Whitelist(WhitelistCommand),
    Blacklist(BlacklistCommand),
    ConnectionLimits(ConnectionLimitsCommand),
    Custom(T),
}

impl<T> From<SwarmCommand> for Command<T> {
    fn from(cmd: SwarmCommand) -> Self {
        Command::Swarm(cmd)
    }
}

#[cfg(feature = "autonat")]
impl<T> From<AutonatCommand> for Command<T> {
    fn from(cmd: AutonatCommand) -> Self {
        Command::Autonat(cmd)
    }
}

#[cfg(feature = "gossipsub")]
impl<T> From<GossipsubCommand> for Command<T> {
    fn from(cmd: GossipsubCommand) -> Self {
        Command::Gossipsub(cmd)
    }
}

#[cfg(feature = "floodsub")]
impl<T> From<FloodsubCommand> for Command<T> {
    fn from(cmd: FloodsubCommand) -> Self {
        Command::Floodsub(cmd)
    }
}

#[cfg(feature = "kad")]
impl<T> From<DHTCommand> for Command<T> {
    fn from(cmd: DHTCommand) -> Self {
        Command::Dht(cmd)
    }
}

#[cfg(feature = "request-response")]
impl<T> From<RequestResponseCommand> for Command<T> {
    fn from(cmd: RequestResponseCommand) -> Self {
        Command::RequestResponse(cmd)
    }
}

#[cfg(feature = "stream")]
impl<T> From<StreamCommand> for Command<T> {
    fn from(cmd: StreamCommand) -> Self {
        Command::Stream(cmd)
    }
}

#[cfg(feature = "rendezvous")]
impl<T> From<RendezvousCommand> for Command<T> {
    fn from(cmd: RendezvousCommand) -> Self {
        Command::Rendezvous(cmd)
    }
}

impl<T> From<WhitelistCommand> for Command<T> {
    fn from(cmd: WhitelistCommand) -> Self {
        Command::Whitelist(cmd)
    }
}

impl<T> From<BlacklistCommand> for Command<T> {
    fn from(cmd: BlacklistCommand) -> Self {
        Command::Blacklist(cmd)
    }
}

impl<T> From<ConnectionLimitsCommand> for Command<T> {
    fn from(cmd: ConnectionLimitsCommand) -> Self {
        Command::ConnectionLimits(cmd)
    }
}

#[derive(Debug)]
pub enum SwarmCommand {
    Dial {
        opt: DialOpts,
        resp: oneshot::Sender<Result<ConnectionId>>,
    },
    IsConnected {
        peer_id: PeerId,
        resp: oneshot::Sender<bool>,
    },
    Disconnect {
        target_type: Either<PeerId, ConnectionId>,
        resp: oneshot::Sender<Result<()>>,
    },
    ConnectedPeers {
        resp: oneshot::Sender<Vec<PeerId>>,
    },
    ListenOn {
        address: Multiaddr,
        resp: oneshot::Sender<Result<ListenerId>>,
    },
    GetListeningAddress {
        id: ListenerId,
        resp: oneshot::Sender<Result<Vec<Multiaddr>>>,
    },
    RemoveListener {
        listener_id: ListenerId,
        resp: oneshot::Sender<Result<()>>,
    },
    AddExternalAddress {
        address: Multiaddr,
        resp: oneshot::Sender<Result<()>>,
    },
    RemoveExternalAddress {
        address: Multiaddr,
        resp: oneshot::Sender<Result<()>>,
    },
    ListExternalAddresses {
        resp: oneshot::Sender<Vec<Multiaddr>>,
    },
    ListListeningAddresses {
        resp: oneshot::Sender<Vec<Multiaddr>>,
    },
    AddPeerAddress {
        peer_id: PeerId,
        address: Multiaddr,
        resp: oneshot::Sender<Result<()>>,
    },
    Listener {
        resp: oneshot::Sender<mpsc::Receiver<ConnectionEvent>>,
    },
}

#[derive(Debug)]
pub enum ConnectionEvent {
    ConnectionEstablished {
        peer_id: PeerId,
        connection_id: ConnectionId,
        endpoint: ConnectedPoint,
        established: u32,
    },
    ConnectionClosed {
        peer_id: PeerId,
        connection_id: ConnectionId,
        endpoint: ConnectedPoint,
        num_established: u32,
    },
}

#[cfg(feature = "floodsub")]
#[derive(Debug)]
pub enum FloodsubCommand {
    Subscribe {
        topic: libp2p::floodsub::Topic,
        resp: oneshot::Sender<Result<()>>,
    },
    Unsubscribe {
        topic: libp2p::floodsub::Topic,
        resp: oneshot::Sender<Result<()>>,
    },
    FloodsubListener {
        topic: libp2p::floodsub::Topic,
        resp: oneshot::Sender<Result<mpsc::Receiver<FloodsubEvent>>>,
    },
    Publish(PubsubFloodsubPublish, oneshot::Sender<Result<()>>),
}

#[cfg(feature = "gossipsub")]
#[derive(Debug)]
pub enum GossipsubCommand {
    Subscribe {
        topic: libp2p::gossipsub::TopicHash,
        resp: oneshot::Sender<Result<()>>,
    },
    Unsubscribe {
        topic: libp2p::gossipsub::TopicHash,
        resp: oneshot::Sender<Result<()>>,
    },
    Subscribed {
        resp: oneshot::Sender<Result<Vec<libp2p::gossipsub::TopicHash>>>,
    },
    Peers {
        topic: libp2p::gossipsub::TopicHash,
        resp: oneshot::Sender<Result<Vec<PeerId>>>,
    },
    GossipsubListener {
        topic: libp2p::gossipsub::TopicHash,
        resp: oneshot::Sender<Result<mpsc::Receiver<GossipsubEvent>>>,
    },
    Publish {
        topic: libp2p::gossipsub::TopicHash,
        data: Bytes,
        resp: oneshot::Sender<Result<()>>,
    },
    ReportMessage {
        peer_id: PeerId,
        message_id: MessageId,
        accept: MessageAcceptance,
        resp: oneshot::Sender<Result<bool>>,
    },
}

#[derive(Debug)]
pub enum ConnectionLimitsCommand {
    Get {
        resp: oneshot::Sender<Result<ConnectionLimits>>,
    },
    Set {
        limits: ConnectionLimits,
        resp: oneshot::Sender<Result<()>>,
    },
}

#[cfg(feature = "floodsub")]
#[derive(Debug)]
pub enum PubsubFloodsubPublish {
    Publish {
        topic: libp2p::floodsub::Topic,
        data: Bytes,
    },
    PublishAny {
        topic: libp2p::floodsub::Topic,
        data: Bytes,
    },
    PublishMany {
        topics: Vec<libp2p::floodsub::Topic>,
        data: Bytes,
    },
    PublishManyAny {
        topics: Vec<libp2p::floodsub::Topic>,
        data: Bytes,
    },
}

#[cfg(feature = "autonat")]
#[derive(Debug)]
pub enum AutonatCommand {
    PublicAddress {
        resp: oneshot::Sender<Result<Option<Multiaddr>>>,
    },
    NatStatus {
        resp: oneshot::Sender<Result<libp2p::autonat::NatStatus>>,
    },
    AddServer {
        peer: PeerId,
        address: Option<Multiaddr>,
        resp: oneshot::Sender<Result<()>>,
    },
    RemoveServer {
        peer: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    Probe {
        address: Multiaddr,
        resp: oneshot::Sender<Result<()>>,
    },
}

#[cfg(feature = "kad")]
#[derive(Debug)]
pub enum DHTCommand {
    FindPeer {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<Vec<PeerInfo>>>,
    },
    Bootstrap {
        lazy: bool,
        resp: oneshot::Sender<Result<()>>,
    },
    Provide {
        key: RecordKey,
        resp: oneshot::Sender<Result<()>>,
    },
    StopProviding {
        key: RecordKey,
        resp: oneshot::Sender<Result<()>>,
    },
    GetProviders {
        key: RecordKey,
        resp: oneshot::Sender<Result<mpsc::Receiver<Result<HashSet<PeerId>>>>>,
    },
    SetDHTMode {
        mode: Option<Mode>,
        resp: oneshot::Sender<Result<()>>,
    },
    DHTMode {
        resp: oneshot::Sender<Result<Mode>>,
    },

    AddAddress {
        peer_id: PeerId,
        addr: Multiaddr,
        resp: oneshot::Sender<Result<()>>,
    },

    Get {
        key: RecordKey,
        resp: oneshot::Sender<Result<mpsc::Receiver<Result<PeerRecord>>>>,
    },
    Put {
        key: RecordKey,
        data: Bytes,
        quorum: Quorum,
        resp: oneshot::Sender<Result<()>>,
    },
    Listener {
        key: Option<RecordKey>,
        resp: oneshot::Sender<Result<mpsc::Receiver<DHTEvent>>>,
    },
}

#[cfg(feature = "request-response")]
#[derive(Debug)]
pub enum RequestResponseCommand {
    SendRequests {
        protocol: Option<StreamProtocol>,
        peers: IndexSet<PeerId>,
        request: Bytes,
        resp: oneshot::Sender<Result<BoxStream<'static, (PeerId, Result<Bytes>)>>>,
    },
    SendRequest {
        protocol: Option<StreamProtocol>,
        peer_id: PeerId,
        request: Bytes,
        resp: oneshot::Sender<Result<BoxFuture<'static, Result<Bytes>>>>,
    },
    SendResponse {
        protocol: Option<StreamProtocol>,
        peer_id: PeerId,
        request_id: InboundRequestId,
        response: Bytes,
        resp: oneshot::Sender<Result<()>>,
    },
    ListenForRequests {
        protocol: Option<StreamProtocol>,
        resp: oneshot::Sender<Result<mpsc::Receiver<(PeerId, InboundRequestId, Bytes)>>>,
    },
}

#[cfg(feature = "stream")]
#[derive(Debug)]
pub enum StreamCommand {
    NewStream {
        protocol: StreamProtocol,
        resp: oneshot::Sender<Result<libp2p_stream::IncomingStreams>>,
    },
    ControlHandle {
        resp: oneshot::Sender<Result<libp2p_stream::Control>>,
    },
}

#[cfg(feature = "rendezvous")]
#[derive(Debug)]
pub enum RendezvousCommand {
    Register {
        namespace: String,
        peer_id: PeerId,
        ttl: Option<u64>,
        resp: oneshot::Sender<Result<()>>,
    },
    Unregister {
        namespace: String,
        peer_id: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    Discover {
        namespace: Option<String>,
        peer_id: PeerId,
        cookie: Option<Cookie>,
        ttl: Option<u64>,
        resp: oneshot::Sender<Result<(Cookie, Vec<(PeerId, Vec<Multiaddr>)>)>>,
    },
}

#[cfg(feature = "kad")]
#[derive(Clone, Debug)]
pub enum DHTEvent {
    PutRecord {
        source: PeerId,
        record: RecordHandle<Record>,
    },
    ProvideRecord {
        record: RecordHandle<ProviderRecord>,
    },
}

#[cfg(feature = "kad")]
impl DHTEvent {
    pub(crate) fn set_record_confirmation(&self, ch: oneshot::Sender<Result<Record>>) -> Self {
        let mut event = self.clone();
        match &mut event {
            DHTEvent::PutRecord { record, .. } => {
                record.confirm.replace(ch);
                event
            }
            _ => unreachable!("DHTEvent::PutRecord called on non-PutRecord"),
        }
    }
    pub(crate) fn set_provider_confirmation(
        &self,
        ch: oneshot::Sender<Result<ProviderRecord>>,
    ) -> Self {
        let mut event = self.clone();
        match &mut event {
            DHTEvent::ProvideRecord { record, .. } => {
                record.confirm.replace(ch);
                event
            }
            _ => unreachable!("DHTEvent::ProvideRecord called on non-ProvideRecord"),
        }
    }
}

#[cfg(feature = "kad")]
#[derive(Debug)]
pub struct RecordHandle<R> {
    /// The underlining record, if available
    /// Note: a record is only provided if configured by kademlia behaviour
    pub record: Option<R>,
    /// Channel used to confirm provided record
    pub confirm: Option<oneshot::Sender<Result<R>>>,
}

#[cfg(feature = "kad")]
impl<R: Clone> Clone for RecordHandle<R> {
    fn clone(&self) -> Self {
        Self {
            record: self.record.clone(),
            confirm: None,
        }
    }
}

#[cfg(feature = "gossipsub")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipsubEvent {
    Subscribed { peer_id: PeerId },
    Unsubscribed { peer_id: PeerId },
    Message { message: GossipsubMessage },
}

#[cfg(feature = "floodsub")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloodsubEvent {
    Subscribed { peer_id: PeerId },
    Unsubscribed { peer_id: PeerId },
    Message { message: FloodsubMessage },
}

#[cfg(feature = "gossipsub")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GossipsubMessage {
    pub message_id: MessageId,
    pub propagated_source: PeerId,
    pub source: Option<PeerId>,
    pub data: Bytes,
    pub sequence_number: Option<u64>,
}

#[cfg(feature = "floodsub")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FloodsubMessage {
    pub source: PeerId,
    pub data: Bytes,
    pub sequence_number: Vec<u8>,
}

#[derive(Debug)]
pub enum WhitelistCommand {
    Add {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    Remove {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    List {
        resp: oneshot::Sender<Result<Vec<PeerId>>>,
    },
}

#[derive(Debug)]
pub enum BlacklistCommand {
    Add {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    Remove {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<()>>,
    },
    List {
        resp: oneshot::Sender<Result<Vec<PeerId>>>,
    },
}
