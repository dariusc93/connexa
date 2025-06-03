use bytes::Bytes;
use either::Either;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use indexmap::IndexSet;
use libp2p::gossipsub::MessageId;
use libp2p::kad::{Mode, PeerInfo, PeerRecord, Quorum, RecordKey};
use libp2p::request_response::InboundRequestId;
use libp2p::swarm::ConnectionId;
use libp2p::swarm::derive_prelude::ListenerId;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use std::collections::HashSet;

type Result<T> = std::io::Result<T>;

#[derive(Debug)]
pub enum Command<T = ()> {
    Swarm(SwarmCommand),
    Pubsub(PubsubCommand),
    Dht(DHTCommand),
    RequestResponse(RequestResponseCommand),
    #[cfg(feature = "stream")]
    Stream(StreamCommand),
    Rendezvous(RendezvousCommand),
    Custom(T),
}

impl<T> From<SwarmCommand> for Command<T> {
    fn from(cmd: SwarmCommand) -> Self {
        Command::Swarm(cmd)
    }
}

impl<T> From<PubsubCommand> for Command<T> {
    fn from(cmd: PubsubCommand) -> Self {
        Command::Pubsub(cmd)
    }
}

impl<T> From<DHTCommand> for Command<T> {
    fn from(cmd: DHTCommand) -> Self {
        Command::Dht(cmd)
    }
}

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

impl<T> From<RendezvousCommand> for Command<T> {
    fn from(cmd: RendezvousCommand) -> Self {
        Command::Rendezvous(cmd)
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
}

#[derive(Debug)]
pub enum PubsubCommand {
    //TODO: Maybe return stream?
    Subscribe {
        topic: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Unsubscribe {
        topic: String,
        resp: oneshot::Sender<Result<()>>,
    },
    Subscribed {
        resp: oneshot::Sender<Result<Vec<String>>>,
    },
    Peers {
        topic: String,
        resp: oneshot::Sender<Result<Vec<PeerId>>>,
    },
    FloodsubListener {
        topic: String,
        resp: oneshot::Sender<Result<mpsc::Receiver<PubsubEvent<FloodsubMessage>>>>,
    },
    GossipsubListener {
        topic: String,
        resp: oneshot::Sender<Result<mpsc::Receiver<PubsubEvent<GossipsubMessage>>>>,
    },
    Publish(PubsubPublishType),
}

#[derive(Debug)]
pub enum PubsubPublishType {
    Gossipsub {
        topic: String,
        data: Bytes,
        resp: oneshot::Sender<Result<()>>,
    },
    Floodsub(PubsubFloodsubPublish, oneshot::Sender<Result<()>>),
}

#[derive(Debug)]
pub enum PubsubFloodsubPublish {
    Publish { topic: String, data: Bytes },
    PublishAny { topic: String, data: Bytes },
    PublishMany { topics: Vec<String>, data: Bytes },
    PublishManyAny { topics: Vec<String>, data: Bytes },
}

#[derive(Debug)]
pub enum DHTCommand {
    FindPeer {
        peer_id: PeerId,
        resp: oneshot::Sender<Result<Vec<PeerInfo>>>,
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
}

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

#[derive(Debug)]
pub enum RendezvousCommand {
    Register {
        namespace: String,
        peer_id: PeerId,
        ttl: Option<i64>,
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
        use_cookie: bool,
        ttl: Option<i64>,
        resp: oneshot::Sender<Result<mpsc::UnboundedReceiver<(PeerId, Vec<Multiaddr>)>>>,
    },
}

#[derive(Debug, Clone)]
pub enum PubsubEvent<M> {
    Subscribed { peer_id: PeerId },
    Unsubscribed { peer_id: PeerId },
    Message { message: M },
}

#[derive(Debug)]
#[non_exhaustive]
pub struct GossipsubMessage {
    pub message_id: MessageId,
    pub propagated_source: PeerId,
    pub source: Option<PeerId>,
    pub data: Bytes,
    pub sequence_number: Option<u64>,
    pub propagate_message: Option<oneshot::Sender<Result<MessageId>>>,
}

impl GossipsubMessage {
    pub fn inject_channel(&self, ch: oneshot::Sender<Result<MessageId>>) -> GossipsubMessage {
        GossipsubMessage {
            message_id: self.message_id.clone(),
            propagated_source: self.propagated_source,
            source: self.source,
            data: self.data.clone(),
            sequence_number: self.sequence_number,
            propagate_message: Some(ch),
        }
    }
}

impl Clone for GossipsubMessage {
    fn clone(&self) -> Self {
        GossipsubMessage {
            message_id: self.message_id.clone(),
            propagated_source: self.propagated_source,
            source: self.source,
            data: self.data.clone(),
            sequence_number: self.sequence_number,
            propagate_message: None,
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FloodsubMessage {
    pub source: PeerId,
    pub data: Bytes,
    pub sequence_number: Vec<u8>,
}
