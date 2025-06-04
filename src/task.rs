use crate::behaviour;
use crate::behaviour::BehaviourEvent;
use crate::types::{
    Command, DHTCommand, FloodsubMessage, GossipsubMessage, PubsubCommand, PubsubEvent,
    PubsubFloodsubPublish, PubsubPublishType, RequestResponseCommand, SwarmCommand,
};

#[cfg(feature = "stream")]
use crate::types::StreamCommand;
use either::Either;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, StreamExt};
use futures_timer::Delay;
use indexmap::IndexMap;
use libp2p::autonat::v1::Event as AutonatV1Event;
use libp2p::autonat::v2::client::Event as AutonatV2ClientEvent;
use libp2p::autonat::v2::server::Event as AutonatV2ServerEvent;
use libp2p::dcutr::Event as DcutrEvent;
use libp2p::floodsub::FloodsubEvent;
use libp2p::gossipsub::Event as GossipsubEvent;
use libp2p::identify::Event as IdentifyEvent;
use libp2p::kad::store::RecordStore;
use libp2p::kad::{
    AddProviderOk, BootstrapError, BootstrapOk, Event as KademliaEvent, GetClosestPeersError,
    GetClosestPeersOk, GetProvidersOk, GetRecordOk, InboundRequest, PeerInfo, PeerRecord,
    ProviderRecord, PutRecordOk, QueryId, QueryResult, Record, RecordKey as Key, RoutingUpdate,
};
use libp2p::mdns::Event as MdnsEvent;
use libp2p::ping::Event as PingEvent;
use libp2p::relay::Event as RelayServerEvent;
use libp2p::relay::client::Event as RelayClientEvent;
use libp2p::rendezvous::client::Event as RendezvousClientEvent;
use libp2p::rendezvous::server::Event as RendezvousServerEvent;
use libp2p::swarm::derive_prelude::ListenerId;
use libp2p::swarm::{ConnectionId, NetworkBehaviour, SwarmEvent};
use libp2p::upnp::Event as UpnpEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use pollable_map::futures::set::FutureSet;
use pollable_map::optional::Optional;
use pollable_map::stream::StreamMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

pub struct ConnexaTask<C: NetworkBehaviour, T = ()>
where
    C: Send,
    C::ToSwarm: Debug,
{
    pub swarm: Optional<Swarm<behaviour::Behaviour<C>>>,
    pub command_receiver: Optional<mpsc::Receiver<Command<T>>>,
    pub custom_task_callback: Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, T) + 'static + Send>,
    pub custom_event_callback:
        Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, C::ToSwarm) + 'static + Send>,
    pub swarm_event_callback: Box<dyn Fn(&SwarmEvent<BehaviourEvent<C>>) + 'static + Send>,

    /// A listener for sending dht records for manual validation
    pub dht_put_record_sender:
        IndexMap<Key, Vec<mpsc::Sender<(Record, oneshot::Sender<std::io::Result<Record>>)>>>,
    pub dht_put_record_receiver:
        StreamMap<Key, FutureSet<oneshot::Receiver<std::io::Result<Record>>>>,

    /// a listener for sending dht provider records for manual validation
    pub dht_provider_record_sender: IndexMap<
        Key,
        Vec<
            mpsc::Sender<(
                ProviderRecord,
                oneshot::Sender<std::io::Result<ProviderRecord>>,
            )>,
        >,
    >,
    pub dht_provider_record_receiver:
        StreamMap<Key, FutureSet<oneshot::Receiver<std::io::Result<ProviderRecord>>>>,

    pub pending_dht_put_record: IndexMap<QueryId, oneshot::Sender<std::io::Result<()>>>,
    pub pending_dht_put_provider_record: IndexMap<QueryId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_dht_get_record: IndexMap<QueryId, mpsc::Sender<std::io::Result<PeerRecord>>>,
    pub pending_dht_get_provider_record:
        IndexMap<QueryId, mpsc::Sender<std::io::Result<HashSet<PeerId>>>>,

    pub pending_dht_find_closest_peer:
        IndexMap<QueryId, oneshot::Sender<std::io::Result<Vec<PeerInfo>>>>,

    pub pubsub_event_sender: Vec<mpsc::Sender<Either<GossipsubEvent, FloodsubEvent>>>,

    pub pending_connection: IndexMap<ConnectionId, oneshot::Sender<std::io::Result<ConnectionId>>>,
    pub pending_disconnection_by_connection_id:
        IndexMap<ConnectionId, oneshot::Sender<std::io::Result<()>>>,
    pub pending_disconnection_by_peer_id: IndexMap<PeerId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_listen_on: IndexMap<ListenerId, oneshot::Sender<std::io::Result<ListenerId>>>,
    pub pending_remove_listener: IndexMap<ListenerId, oneshot::Sender<std::io::Result<()>>>,

    // pub pending_add_external_address: IndexMap<Multiaddr, oneshot::Sender<std::io::Result<()>>>,
    pub pending_remove_external_address: IndexMap<Multiaddr, oneshot::Sender<std::io::Result<()>>>,

    pub pending_add_peer_address:
        IndexMap<(PeerId, Multiaddr), oneshot::Sender<std::io::Result<()>>>,

    pub gossipsub_listener:
        IndexMap<libp2p::gossipsub::TopicHash, Vec<mpsc::Sender<PubsubEvent<GossipsubMessage>>>>,
    pub floodsub_listener:
        IndexMap<libp2p::floodsub::Topic, Vec<mpsc::Sender<PubsubEvent<FloodsubMessage>>>>,

    pub cleanup_timer: Delay,
    pub cleanup_interval: Duration,
}

impl<C: NetworkBehaviour, T> ConnexaTask<C, T>
where
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn new(swarm: Swarm<behaviour::Behaviour<C>>) -> Self {
        let duration = Duration::from_secs(10);
        Self {
            swarm: Optional::new(swarm),
            command_receiver: Optional::default(),
            custom_event_callback: Box::new(|_, _| ()),
            custom_task_callback: Box::new(|_, _| ()),
            swarm_event_callback: Box::new(|_| ()),
            dht_put_record_sender: IndexMap::new(),
            dht_put_record_receiver: StreamMap::new(),
            dht_provider_record_sender: IndexMap::new(),
            dht_provider_record_receiver: StreamMap::new(),
            pending_dht_put_record: Default::default(),
            pending_dht_put_provider_record: IndexMap::new(),
            pending_dht_get_record: Default::default(),
            pending_dht_get_provider_record: Default::default(),
            pending_dht_find_closest_peer: Default::default(),
            pubsub_event_sender: Vec::new(),
            cleanup_timer: Delay::new(duration),
            cleanup_interval: duration,
            pending_connection: IndexMap::new(),
            pending_disconnection_by_peer_id: IndexMap::new(),
            pending_disconnection_by_connection_id: IndexMap::new(),
            pending_listen_on: IndexMap::new(),
            pending_remove_listener: IndexMap::new(),
            // pending_add_external_address: IndexMap::new(),
            pending_remove_external_address: IndexMap::new(),
            pending_add_peer_address: IndexMap::new(),
            floodsub_listener: Default::default(),
            gossipsub_listener: Default::default(),
        }
    }

    pub fn set_command_receiver(&mut self, command_receiver: mpsc::Receiver<Command<T>>) {
        self.command_receiver.replace(command_receiver);
    }

    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, C::ToSwarm) + Send + 'static,
    {
        self.custom_event_callback = Box::new(callback);
    }

    pub fn set_task_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, T) + Send + 'static,
    {
        self.custom_task_callback = Box::new(callback);
    }

    pub fn set_swarm_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(&SwarmEvent<BehaviourEvent<C>>) + Send + 'static,
    {
        self.swarm_event_callback = Box::new(callback);
    }

    pub fn process_command(&mut self, command: Command<T>) {
        let Some(swarm) = self.swarm.as_mut() else {
            return;
        };
        match command {
            Command::Swarm(swarm_command) => match swarm_command {
                SwarmCommand::Dial { opt, resp } => {
                    let connection_id = opt.connection_id();
                    if let Err(e) = swarm.dial(opt) {
                        let _ = resp.send(Err(std::io::Error::other(e)));
                        return;
                    }
                    self.pending_connection.insert(connection_id, resp);
                }
                SwarmCommand::IsConnected { peer_id, resp } => {
                    let is_connected = swarm.is_connected(&peer_id);
                    let _ = resp.send(is_connected);
                }
                SwarmCommand::Disconnect { target_type, resp } => match target_type {
                    Either::Left(peer_id) => {
                        if swarm.disconnect_peer_id(peer_id).is_err() {
                            let _ = resp.send(Err(std::io::Error::other("peer is not connected")));
                            return;
                        }
                        self.pending_disconnection_by_peer_id.insert(peer_id, resp);
                    }
                    Either::Right(connection_id) => {
                        if !swarm.close_connection(connection_id) {
                            let _ = resp.send(Err(std::io::Error::other("not a valid connection")));
                            return;
                        }
                        self.pending_disconnection_by_connection_id
                            .insert(connection_id, resp);
                    }
                },
                SwarmCommand::ConnectedPeers { resp } => {
                    let connected_peers = swarm.connected_peers();
                    let _ = resp.send(connected_peers.copied().collect());
                }
                SwarmCommand::ListenOn { address, resp } => {
                    let id = match swarm.listen_on(address) {
                        Ok(id) => id,
                        Err(e) => {
                            let _ = resp.send(Err(std::io::Error::other(e)));
                            return;
                        }
                    };
                    self.pending_listen_on.insert(id, resp);
                }
                SwarmCommand::RemoveListener { listener_id, resp } => {
                    if !swarm.remove_listener(listener_id) {
                        let _ = resp.send(Err(std::io::Error::other("listener not found")));
                        return;
                    }
                    self.pending_remove_listener.insert(listener_id, resp);
                }
                SwarmCommand::AddExternalAddress { address, resp } => {
                    // let addr = address.clone();
                    swarm.add_external_address(address);
                    // self.pending_add_external_address.insert(addr, resp);
                    let _ = resp.send(Ok(()));
                }
                SwarmCommand::RemoveExternalAddress { address, resp } => {
                    swarm.remove_external_address(&address);
                    self.pending_remove_external_address.insert(address, resp);
                }
                SwarmCommand::ListExternalAddresses { resp } => {
                    let addresses = swarm.external_addresses().cloned().collect();
                    let _ = resp.send(addresses);
                }
                SwarmCommand::ListListeningAddresses { resp } => {
                    let addresses = swarm.listeners().cloned().collect();
                    let _ = resp.send(addresses);
                }
                SwarmCommand::AddPeerAddress {
                    peer_id,
                    address,
                    resp,
                } => {
                    let addr = address.clone();
                    swarm.add_peer_address(peer_id, address);
                    self.pending_add_peer_address.insert((peer_id, addr), resp);
                }
            },
            Command::Pubsub(pubsub_command) => match pubsub_command {
                PubsubCommand::Subscribe { topic, resp } => {
                    match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(gossipsub) => {
                            let Some(pubsub) = gossipsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("gossipsub is not enabled")));
                                return;
                            };
                            let topic = libp2p::gossipsub::IdentTopic::new(topic);
                            match pubsub.subscribe(&topic) {
                                Ok(true) => {
                                    let _ = resp.send(Ok(()));
                                }
                                Ok(false) => {
                                    let _ = resp.send(Err(std::io::Error::other(
                                        "topic already subscribed",
                                    )));
                                }
                                Err(e) => {
                                    let _ = resp.send(Err(std::io::Error::other(e)));
                                }
                            }
                        }
                        Either::Right(floodsub) => {
                            let Some(pubsub) = floodsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("floodsub is not enabled")));
                                return;
                            };
                            let topic = libp2p::floodsub::Topic::new(topic);
                            match pubsub.subscribe(topic) {
                                true => {
                                    let _ = resp.send(Ok(()));
                                }
                                false => {
                                    let _ = resp.send(Err(std::io::Error::other(
                                        "topic already subscribed",
                                    )));
                                }
                            }
                        }
                    }
                }
                PubsubCommand::Unsubscribe { topic, resp } => {
                    match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(gossipsub) => {
                            let Some(pubsub) = gossipsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("gossipsub is not enabled")));
                                return;
                            };
                            let topic = libp2p::gossipsub::IdentTopic::new(topic);
                            match pubsub.unsubscribe(&topic) {
                                true => {
                                    let _ = resp.send(Ok(()));
                                }
                                false => {
                                    let _ = resp.send(Err(std::io::Error::other(
                                        "not subscribed to topic",
                                    )));
                                }
                            }
                        }
                        Either::Right(floodsub) => {
                            let Some(pubsub) = floodsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("floodsub is not enabled")));
                                return;
                            };
                            let topic = libp2p::floodsub::Topic::new(topic);
                            match pubsub.unsubscribe(topic) {
                                true => {
                                    let _ = resp.send(Ok(()));
                                }
                                false => {
                                    let _ = resp.send(Err(std::io::Error::other(
                                        "not subscribed to topic",
                                    )));
                                }
                            }
                        }
                    }
                }
                PubsubCommand::Subscribed { resp } => match swarm.behaviour_mut().pubsub.as_mut() {
                    Either::Left(gossipsub) => {
                        let Some(pubsub) = gossipsub.as_mut() else {
                            let _ =
                                resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                            return;
                        };
                        let topics = pubsub.topics().map(|topic| topic.to_string()).collect();

                        let _ = resp.send(Ok(topics));
                    }
                    Either::Right(floodsub) => {
                        if !floodsub.is_enabled() {
                            let _ =
                                resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                            return;
                        };

                        let _ = resp.send(Err(std::io::Error::other(
                            "function is unimplemented at this time",
                        )));
                    }
                },
                PubsubCommand::Peers { topic, resp } => {
                    match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(gossipsub) => {
                            let Some(pubsub) = gossipsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("gossipsub is not enabled")));
                                return;
                            };
                            let topic = libp2p::gossipsub::IdentTopic::new(topic).hash();
                            let peers = pubsub.mesh_peers(&topic).copied().collect();

                            let _ = resp.send(Ok(peers));
                        }
                        Either::Right(floodsub) => {
                            if !floodsub.is_enabled() {
                                let _ = resp
                                    .send(Err(std::io::Error::other("floodsub is not enabled")));
                                return;
                            };

                            let _ = resp.send(Err(std::io::Error::other(
                                "function is unimplemented at this time",
                            )));
                        }
                    }
                }
                PubsubCommand::Publish(PubsubPublishType::Gossipsub { topic, data, resp }) => {
                    let pubsub = match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(gossipsub) => {
                            let Some(pubsub) = gossipsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("gossipsub is not enabled")));
                                return;
                            };

                            pubsub
                        }
                        Either::Right(_) => {
                            unreachable!()
                        }
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic);
                    let ret = match pubsub.publish(topic, data) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(std::io::Error::other(e)),
                    };

                    let _ = resp.send(ret);
                }
                PubsubCommand::Publish(PubsubPublishType::Floodsub(pubsub_type, resp)) => {
                    let pubsub = match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(_) => {
                            unreachable!()
                        }
                        Either::Right(floodsub) => {
                            let Some(pubsub) = floodsub.as_mut() else {
                                let _ = resp
                                    .send(Err(std::io::Error::other("floodsub is not enabled")));
                                return;
                            };

                            pubsub
                        }
                    };

                    match pubsub_type {
                        PubsubFloodsubPublish::Publish { topic, data } => {
                            let topic = libp2p::floodsub::Topic::new(topic);
                            pubsub.publish(topic, data);
                        }
                        PubsubFloodsubPublish::PublishAny { topic, data } => {
                            let topic = libp2p::floodsub::Topic::new(topic);
                            pubsub.publish_any(topic, data);
                        }
                        PubsubFloodsubPublish::PublishMany { topics, data } => {
                            let topics = topics.into_iter().map(libp2p::floodsub::Topic::new);
                            pubsub.publish_many(topics, data);
                        }
                        PubsubFloodsubPublish::PublishManyAny { topics, data } => {
                            let topics = topics.into_iter().map(libp2p::floodsub::Topic::new);
                            pubsub.publish_many_any(topics, data);
                        }
                    }

                    let _ = resp.send(Ok(()));
                }
                PubsubCommand::FloodsubListener { topic, resp } => {
                    match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(_) => {
                            unreachable!()
                        }
                        Either::Right(floodsub) => {
                            if !floodsub.is_enabled() {
                                let _ = resp
                                    .send(Err(std::io::Error::other("floodsub is not enabled")));
                                return;
                            }
                        }
                    };

                    let topic = libp2p::floodsub::Topic::new(topic);

                    let (tx, rx) = mpsc::channel(50);

                    self.floodsub_listener.entry(topic).or_default().push(tx);

                    let _ = resp.send(Ok(rx));
                }
                PubsubCommand::GossipsubListener { topic, resp } => {
                    match swarm.behaviour_mut().pubsub.as_mut() {
                        Either::Left(gossipsub) => {
                            if !gossipsub.is_enabled() {
                                let _ = resp
                                    .send(Err(std::io::Error::other("gossipsub is not enabled")));
                                return;
                            }
                        }
                        Either::Right(_) => {
                            unreachable!()
                        }
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic).hash();

                    let (tx, rx) = mpsc::channel(50);

                    self.gossipsub_listener.entry(topic).or_default().push(tx);

                    let _ = resp.send(Ok(rx));
                }
            },
            Command::Dht(dht_command) => match dht_command {
                DHTCommand::FindPeer { peer_id, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let id = kad.get_closest_peers(peer_id);

                    self.pending_dht_find_closest_peer.insert(id, resp);
                }
                DHTCommand::Provide { key, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let id = match kad.start_providing(key.clone()) {
                        Ok(id) => id,
                        Err(e) => {
                            let _ = resp.send(Err(std::io::Error::other(e)));
                            return;
                        }
                    };

                    self.pending_dht_put_provider_record.insert(id, resp);
                }
                DHTCommand::StopProviding { key, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    kad.stop_providing(&key);

                    let _ = resp.send(Ok(()));
                }
                DHTCommand::GetProviders { key, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let id = kad.get_providers(key);

                    let (tx, rx) = mpsc::channel(10);

                    self.pending_dht_get_provider_record.insert(id, tx);

                    let _ = resp.send(Ok(rx));
                }
                DHTCommand::SetDHTMode { mode, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    //TODO: Maybe check for mode change that is emitted?
                    kad.set_mode(mode);

                    let _ = resp.send(Ok(()));
                }
                DHTCommand::DHTMode { resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let _ = resp.send(Ok(kad.mode()));
                }
                DHTCommand::AddAddress {
                    peer_id,
                    addr,
                    resp,
                } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let routing = kad.add_address(&peer_id, addr);
                    // TODO: If its pending then maybe we should wait until there is some status update?
                    let ret = match routing {
                        RoutingUpdate::Success | RoutingUpdate::Pending => Ok(()),
                        RoutingUpdate::Failed => {
                            Err(std::io::Error::other("failed to add peer to routing table"))
                        }
                    };

                    let _ = resp.send(ret);
                }
                DHTCommand::Get { key, resp } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let id = kad.get_record(key.clone());

                    let (tx, rx) = mpsc::channel(10);

                    self.pending_dht_get_record.insert(id, tx);

                    let _ = resp.send(Ok(rx));
                }
                DHTCommand::Put {
                    key,
                    data,
                    quorum,
                    resp,
                } => {
                    let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    };

                    let record = Record::new(key, data.into());

                    let id = match kad.put_record(record, quorum) {
                        Ok(id) => id,
                        Err(e) => {
                            let _ = resp.send(Err(std::io::Error::other(e)));
                            return;
                        }
                    };

                    self.pending_dht_put_record.insert(id, resp);
                }
            },
            #[cfg(feature = "stream")]
            Command::Stream(stream_command) => match stream_command {
                StreamCommand::NewStream { protocol, resp } => {
                    let Some(stream) = swarm.behaviour_mut().stream.as_mut() else {
                        let _ =
                            resp.send(Err(std::io::Error::other("stream protocol is not enabled")));
                        return;
                    };

                    let _ = resp.send(
                        stream
                            .new_control()
                            .accept(protocol)
                            .map_err(std::io::Error::other),
                    );
                }
                StreamCommand::ControlHandle { resp } => {
                    let Some(stream) = swarm.behaviour_mut().stream.as_mut() else {
                        let _ =
                            resp.send(Err(std::io::Error::other("stream protocol is not enabled")));
                        return;
                    };

                    let _ = resp.send(Ok(stream.new_control()));
                }
            },
            Command::RequestResponse(request_response_command) => match request_response_command {
                RequestResponseCommand::SendRequests {
                    protocol,
                    peers,
                    request,
                    resp,
                } => {
                    let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                        let _ = resp.send(Err(std::io::Error::other(
                            "request response protocol is not enabled",
                        )));
                        return;
                    };

                    let st = rr.send_requests(peers, request);
                    let _ = resp.send(Ok(st));
                }
                RequestResponseCommand::SendRequest {
                    protocol,
                    peer_id,
                    request,
                    resp,
                } => {
                    let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                        let _ = resp.send(Err(std::io::Error::other(
                            "request response protocol is not enabled",
                        )));
                        return;
                    };

                    let fut = rr.send_request(peer_id, request);
                    let _ = resp.send(Ok(fut));
                }
                RequestResponseCommand::SendResponse {
                    protocol,
                    peer_id,
                    request_id,
                    response,
                    resp,
                } => {
                    let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                        let _ = resp.send(Err(std::io::Error::other(
                            "request response protocol is not enabled",
                        )));
                        return;
                    };

                    let ret = rr.send_response(peer_id, request_id, response);

                    let _ = resp.send(ret);
                }
                RequestResponseCommand::ListenForRequests { protocol, resp } => {
                    let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                        let _ = resp.send(Err(std::io::Error::other(
                            "request response protocol is not enabled",
                        )));
                        return;
                    };
                    let rx = rr.subscribe();
                    let _ = resp.send(Ok(rx));
                }
            },
            Command::Rendezvous(rendezvous_command) => {
                // TODO
                let _ = rendezvous_command;
            }
            Command::Custom(custom_command) => {
                (self.custom_task_callback)(swarm, custom_command);
            }
        }
    }

    pub fn process_swarm_event(&mut self, event: SwarmEvent<behaviour::BehaviourEvent<C>>) {
        (self.swarm_event_callback)(&event);
        match event {
            SwarmEvent::Behaviour(event) => self.process_swarm_behaviour_event(event),
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                concurrent_dial_errors: _,
                established_in,
            } => {
                tracing::info!(%peer_id, %connection_id, ?endpoint, %num_established, ?established_in, "connection established");
                if let Some(sender) = self.pending_connection.shift_remove(&connection_id) {
                    let _ = sender.send(Ok(connection_id));
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                cause,
            } => {
                tracing::info!(%peer_id, %connection_id, ?endpoint, %num_established, ?cause, "connection closed");
                let pending_ch_by_connection_id = self
                    .pending_disconnection_by_connection_id
                    .shift_remove(&connection_id);
                let pending_ch_by_peer_id =
                    self.pending_disconnection_by_peer_id.shift_remove(&peer_id);
                let ret = match cause {
                    Some(e) => Err(std::io::Error::other(e)),
                    None => Ok(()),
                };

                match (pending_ch_by_connection_id, pending_ch_by_peer_id) {
                    (Some(ch), None) => {
                        let _ = ch.send(ret);
                    }
                    (None, Some(ch)) => {
                        let _ = ch.send(ret);
                    }
                    (Some(ch_left), Some(ch_right)) => {
                        // Since there is an attempt to disconnect the peer as well, we will respond to both pending request with the peer containing the "cause", if any.
                        let _ = ch_left.send(Ok(()));
                        let _ = ch_right.send(ret);
                    }
                    (None, None) => {}
                }
            }
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::OutgoingConnectionError {
                connection_id,
                peer_id,
                error,
            } => {
                tracing::error!(%connection_id, ?peer_id, error=%error, "outgoing connection error");
                if let Some(sender) = self.pending_connection.shift_remove(&connection_id) {
                    let _ = sender.send(Err(std::io::Error::other(error)));
                }
            }
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } => {
                tracing::info!(%listener_id, %address, "new listen address");
                if let Some(ch) = self.pending_listen_on.shift_remove(&listener_id) {
                    let _ = ch.send(Ok(listener_id));
                }
            }
            SwarmEvent::ExpiredListenAddr {
                listener_id,
                address,
            } => {
                // TODO: Determine if we should remove the address from external addresses
                tracing::info!(%listener_id, %address, "expired listen address");
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                reason,
            } => {
                tracing::info!(%listener_id, ?addresses, ?reason, "listener closed");
                if let Some(ch) = self.pending_remove_listener.shift_remove(&listener_id) {
                    let _ = ch.send(reason.map_err(std::io::Error::other));
                }
            }
            SwarmEvent::ListenerError { listener_id, error } => {
                tracing::error!(%listener_id, error=%error, "listener error");
                if let Some(ch) = self.pending_listen_on.shift_remove(&listener_id) {
                    let _ = ch.send(Err(std::io::Error::other(error)));
                }
            }
            SwarmEvent::Dialing { .. } => {}
            SwarmEvent::NewExternalAddrCandidate { .. } => {}
            SwarmEvent::ExternalAddrConfirmed { address } => {
                tracing::debug!(%address, "external address confirmed");
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                if let Some(ch) = self.pending_remove_external_address.shift_remove(&address) {
                    let _ = ch.send(Ok(()));
                }
            }
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                if let Some(ch) = self
                    .pending_add_peer_address
                    .shift_remove(&(peer_id, address))
                {
                    let _ = ch.send(Ok(()));
                }
            }
            _ => {}
        }
    }

    pub fn process_swarm_behaviour_event(&mut self, event: BehaviourEvent<C>) {
        let Some(swarm) = self.swarm.as_mut() else {
            return;
        };

        match event {
            BehaviourEvent::Relay(event) => self.process_relay_server_event(event),
            BehaviourEvent::RelayClient(event) => self.process_relay_client_event(event),
            BehaviourEvent::Upnp(event) => self.process_upnp_event(event),
            BehaviourEvent::Dcutr(event) => self.process_dcutr_event(event),
            BehaviourEvent::RendezvousClient(event) => self.process_rendezvous_client_event(event),
            BehaviourEvent::RendezvousServer(event) => self.process_rendezvous_server_event(event),
            BehaviourEvent::Mdns(event) => self.process_mdns_event(event),
            BehaviourEvent::Pubsub(ev) => match ev {
                Either::Left(gossipsub_event) => self.process_gossipsub_event(gossipsub_event),
                Either::Right(floodsub_event) => self.process_floodsub_event(floodsub_event),
            },
            BehaviourEvent::Kademlia(event) => self.process_kademlia_event(event),
            BehaviourEvent::Identify(event) => self.process_identify_event(event),
            BehaviourEvent::Ping(event) => self.process_ping_event(event),
            BehaviourEvent::AutonatV1(event) => self.process_autonat_v1_event(event),
            BehaviourEvent::AutonatV2Client(event) => self.process_autonat_v2_client_event(event),
            BehaviourEvent::AutonatV2Server(event) => self.process_autonat_v2_server_event(event),
            BehaviourEvent::Custom(custom_event) => {
                (self.custom_event_callback)(swarm, custom_event)
            }
            _ => unreachable!(),
        }
    }

    pub fn process_rendezvous_client_event(&mut self, event: RendezvousClientEvent) {
        match event {
            RendezvousClientEvent::Discovered { .. } => {}
            RendezvousClientEvent::DiscoverFailed { .. } => {}
            RendezvousClientEvent::Registered { .. } => {}
            RendezvousClientEvent::RegisterFailed { .. } => {}
            RendezvousClientEvent::Expired { .. } => {}
        }
    }

    pub fn process_rendezvous_server_event(&mut self, event: RendezvousServerEvent) {
        match event {
            RendezvousServerEvent::DiscoverServed { .. } => {}
            RendezvousServerEvent::DiscoverNotServed { .. } => {}
            RendezvousServerEvent::PeerRegistered { .. } => {}
            RendezvousServerEvent::PeerNotRegistered { .. } => {}
            RendezvousServerEvent::PeerUnregistered { .. } => {}
            RendezvousServerEvent::RegistrationExpired(_) => {}
        }
    }

    pub fn process_upnp_event(&mut self, event: UpnpEvent) {
        match event {
            UpnpEvent::NewExternalAddr(addr) => {
                tracing::info!(?addr, "upnp external address discovered");
            }
            UpnpEvent::ExpiredExternalAddr(addr) => {
                tracing::info!(?addr, "upnp external address expired");
            }
            UpnpEvent::GatewayNotFound => {
                tracing::warn!("upnp gateway not found");
            }
            UpnpEvent::NonRoutableGateway => {
                tracing::warn!("upnp gateway is not routable");
            }
        }
    }

    pub fn process_gossipsub_event(&mut self, event: GossipsubEvent) {
        let (topic, event) = match event {
            GossipsubEvent::Message {
                propagation_source,
                message,
                message_id,
            } => {
                let topic = message.topic;
                let message = GossipsubMessage {
                    message_id,
                    propagated_source: propagation_source,
                    source: message.source,
                    sequence_number: message.sequence_number,
                    data: message.data.into(),
                    propagate_message: None,
                };

                let event = PubsubEvent::Message { message };

                (topic, event)
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                let event = PubsubEvent::Subscribed { peer_id };
                (topic, event)
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                let event = PubsubEvent::Unsubscribed { peer_id };
                (topic, event)
            }
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                tracing::info!(%peer_id, "peer does not support gossipsub");
                return;
            }
            GossipsubEvent::SlowPeer {
                peer_id,
                failed_messages,
            } => {
                tracing::info!(%peer_id, ?failed_messages, "peer is slow");
                return;
            }
        };

        let Some(chs) = self.gossipsub_listener.get_mut(&topic) else {
            return;
        };

        // Should we retain the list too for any closed channels?
        for ch in chs {
            // TODO: Perform a check to see if the message needs propagation?
            let _ = ch.try_send(event.clone());
        }
    }

    pub fn process_floodsub_event(&mut self, event: FloodsubEvent) {
        let (topics, event) = match event {
            FloodsubEvent::Message(libp2p::floodsub::FloodsubMessage {
                source,
                data,
                sequence_number,
                topics,
            }) => {
                let message = FloodsubMessage {
                    source,
                    data,
                    sequence_number,
                };

                let event = PubsubEvent::Message { message };

                (topics, event)
            }
            FloodsubEvent::Subscribed { peer_id, topic } => {
                let event = PubsubEvent::Subscribed { peer_id };
                (vec![topic], event)
            }
            FloodsubEvent::Unsubscribed { peer_id, topic } => {
                let event = PubsubEvent::Unsubscribed { peer_id };
                (vec![topic], event)
            }
        };

        for topic in topics {
            let Some(chs) = self.floodsub_listener.get_mut(&topic) else {
                continue;
            };

            // Should we retain the list too for any closed channels?

            for ch in chs {
                let _ = ch.try_send(event.clone());
            }
        }
    }

    pub fn process_autonat_v1_event(&mut self, event: AutonatV1Event) {
        match event {
            AutonatV1Event::InboundProbe(_) => {}
            AutonatV1Event::OutboundProbe(_) => {}
            AutonatV1Event::StatusChanged { .. } => {}
        }
    }

    pub fn process_autonat_v2_client_event(&mut self, event: AutonatV2ClientEvent) {
        let AutonatV2ClientEvent {
            tested_addr,
            bytes_sent,
            server,
            result,
        } = event;
        match result {
            Ok(_) => {
                tracing::info!(%tested_addr, %bytes_sent, %server, "autonat v2 response succeed");
            }
            Err(e) => {
                tracing::info!(%tested_addr, %bytes_sent, %server, error=%e, "autonat v2 response error");
            }
        }
    }

    pub fn process_autonat_v2_server_event(&mut self, event: AutonatV2ServerEvent) {
        let AutonatV2ServerEvent {
            all_addrs,
            tested_addr,
            client,
            data_amount,
            result,
        } = event;
        match result {
            Ok(_) => {
                tracing::info!(%tested_addr, ?all_addrs, %client, %data_amount, "autonat v2 response succeed");
            }
            Err(e) => {
                tracing::info!(%tested_addr, ?all_addrs, %client, %data_amount, error=%e, "autonat v2 response error");
            }
        }
    }

    pub fn process_ping_event(&mut self, event: PingEvent) {
        let PingEvent {
            peer,
            connection,
            result,
        } = event;
        match result {
            Ok(duration) => {
                tracing::info!("ping to {} at {} took {:?}", peer, connection, duration);
            }
            Err(e) => {
                tracing::error!("ping to {} at {} failed: {:?}", peer, connection, e);
            }
        }
    }

    pub fn process_identify_event(&mut self, event: IdentifyEvent) {
        match event {
            IdentifyEvent::Received {
                peer_id,
                connection_id,
                info,
            } => {
                tracing::info!(%peer_id, %connection_id, ?info, "identify received");
            }
            IdentifyEvent::Sent {
                peer_id,
                connection_id,
            } => {
                tracing::info!(%peer_id, %connection_id, "identify sent");
            }
            IdentifyEvent::Pushed {
                peer_id,
                connection_id,
                info,
            } => {
                tracing::info!(%peer_id, %connection_id, ?info, "identify pushed");
            }
            IdentifyEvent::Error {
                peer_id,
                connection_id,
                error,
            } => {
                tracing::error!(%peer_id, %connection_id, error=%error, "identify error");
            }
        }
    }

    pub fn process_kademlia_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::InboundRequest { request } => match request {
                InboundRequest::FindNode { num_closer_peers } => {
                    tracing::trace!(?num_closer_peers, "kademlia find node request");
                }
                InboundRequest::GetProvider {
                    num_closer_peers,
                    num_provider_peers,
                } => {
                    tracing::trace!(
                        ?num_closer_peers,
                        ?num_provider_peers,
                        "kademlia get provider request"
                    );
                }
                InboundRequest::AddProvider { record } => {
                    let Some(provider_record) = record else {
                        return;
                    };

                    tracing::trace!(?provider_record, "kademlia add provider request");
                    let key = provider_record.key.clone();
                    let Some(sender) = self.dht_provider_record_sender.get_mut(&key) else {
                        return;
                    };

                    sender.retain(|ch_sender| !ch_sender.is_closed());
                    for ch_sender in sender.iter_mut() {
                        let (tx, rx) = oneshot::channel();
                        let _ = ch_sender.try_send((provider_record.clone(), tx));
                        // TODO: Implement a `get_mut_or_default` into pollable-map
                        let set = match self.dht_provider_record_receiver.get_mut(&key) {
                            Some(set) => set,
                            None => {
                                self.dht_provider_record_receiver
                                    .insert(key.clone(), FutureSet::new());
                                self.dht_provider_record_receiver
                                    .get_mut(&key)
                                    .expect("entry exist in map")
                            }
                        };

                        set.insert(rx);
                    }
                }
                InboundRequest::GetRecord {
                    num_closer_peers,
                    present_locally,
                } => {
                    tracing::trace!(
                        ?num_closer_peers,
                        ?present_locally,
                        "kademlia get record request"
                    );
                }
                InboundRequest::PutRecord {
                    source,
                    connection,
                    record,
                } => {
                    tracing::trace!(?source, ?connection, "kademlia put record request");
                    let Some(record) = record else {
                        return;
                    };

                    tracing::trace!(?record, "kademlia put record request");
                    let key = record.key.clone();
                    let Some(sender) = self.dht_put_record_sender.get_mut(&key) else {
                        return;
                    };

                    sender.retain(|ch_sender| !ch_sender.is_closed());
                    for ch_sender in sender.iter_mut() {
                        let (tx, rx) = oneshot::channel();
                        let _ = ch_sender.try_send((record.clone(), tx));

                        // TODO: Implement a `get_mut_or_default` into pollable-map
                        let set = match self.dht_put_record_receiver.get_mut(&key) {
                            Some(set) => set,
                            None => {
                                self.dht_put_record_receiver
                                    .insert(key.clone(), FutureSet::new());
                                self.dht_put_record_receiver
                                    .get_mut(&key)
                                    .expect("entry exist in map")
                            }
                        };

                        set.insert(rx);
                    }
                }
            },
            KademliaEvent::OutboundQueryProgressed {
                id,
                result,
                stats: _,
                // should we check for the final event?
                step: _,
            } => match result {
                QueryResult::Bootstrap(result) => match result {
                    Ok(BootstrapOk {
                        peer,
                        num_remaining,
                    }) => {
                        tracing::info!(?peer, ?num_remaining, "kademlia bootstrap");
                    }
                    Err(BootstrapError::Timeout {
                        peer,
                        num_remaining,
                    }) => {
                        tracing::info!(?peer, ?num_remaining, "kademlia bootstrap timeout");
                    }
                },
                QueryResult::GetClosestPeers(result) => match result {
                    Ok(GetClosestPeersOk { key, peers }) => {
                        tracing::info!(?key, ?peers, "kademlia get closest peers");
                    }
                    Err(GetClosestPeersError::Timeout { key, peers }) => {
                        tracing::info!(?key, ?peers, "kademlia get closest peers timeout");
                    }
                },
                QueryResult::GetProviders(result) => match result {
                    Ok(GetProvidersOk::FoundProviders { key, providers }) => {
                        tracing::info!(?key, ?providers, "kademlia found providers");
                        if let Some(ch) = self.pending_dht_get_provider_record.get_mut(&id) {
                            let _ = ch.try_send(Ok(providers));
                        }
                    }
                    Ok(GetProvidersOk::FinishedWithNoAdditionalRecord { closest_peers: _ }) => {
                        if let Some(mut ch) = self.pending_dht_get_provider_record.shift_remove(&id)
                        {
                            ch.close_channel();
                        }
                    }
                    Err(e) => {
                        tracing::error!(%e, "error getting providers");
                        if let Some(mut ch) = self.pending_dht_get_provider_record.shift_remove(&id)
                        {
                            let _ = ch.try_send(Err(std::io::Error::other(e)));
                        }
                    }
                },
                QueryResult::StartProviding(result) => match result {
                    Ok(AddProviderOk { key }) => {
                        tracing::info!(?key, "kademlia start providing");
                        if let Some(ch) = self.pending_dht_put_provider_record.shift_remove(&id) {
                            let _ = ch.send(Ok(()));
                        }
                    }
                    Err(e) => {
                        if let Some(ch) = self.pending_dht_put_provider_record.shift_remove(&id) {
                            let _ = ch.send(Err(std::io::Error::other(e)));
                        }
                    }
                },
                QueryResult::RepublishProvider(result) => match result {
                    Ok(AddProviderOk { key }) => {
                        tracing::info!(?key, "kademlia republish provider record");
                    }
                    Err(e) => {
                        tracing::error!(%e, "error while republishing provider record");
                    }
                },
                QueryResult::GetRecord(result) => match result {
                    Ok(GetRecordOk::FoundRecord(record)) => {
                        tracing::info!(?record, "kademlia get record found");
                        if let Some(ch) = self.pending_dht_get_record.get_mut(&id) {
                            let _ = ch.try_send(Ok(record));
                        }
                    }
                    Ok(GetRecordOk::FinishedWithNoAdditionalRecord {
                        cache_candidates: _,
                    }) => {
                        if let Some(mut ch) = self.pending_dht_get_record.shift_remove(&id) {
                            ch.close_channel();
                        }
                    }
                    Err(e) => {
                        if let Some(mut ch) = self.pending_dht_get_record.shift_remove(&id) {
                            let _ = ch.try_send(Err(std::io::Error::other(e)));
                        }
                    }
                },
                QueryResult::PutRecord(result) => match result {
                    Ok(PutRecordOk { key }) => {
                        tracing::info!(?key, "kademlia put record");
                        if let Some(ch) = self.pending_dht_put_record.shift_remove(&id) {
                            let _ = ch.send(Ok(()));
                        }
                    }
                    Err(e) => {
                        tracing::error!(%e, "kademlia put record error");
                        if let Some(ch) = self.pending_dht_put_record.shift_remove(&id) {
                            let _ = ch.send(Err(std::io::Error::other(e)));
                        }
                    }
                },
                QueryResult::RepublishRecord(result) => match result {
                    Ok(PutRecordOk { key }) => {
                        tracing::trace!(?key, "record was republished");
                    }
                    Err(e) => {
                        tracing::error!(%e, "error while republishing record");
                    }
                },
            },
            KademliaEvent::RoutingUpdated {
                peer,
                is_new_peer,
                addresses,
                bucket_range,
                old_peer,
            } => {
                tracing::trace!(
                    ?peer,
                    ?is_new_peer,
                    ?addresses,
                    ?bucket_range,
                    ?old_peer,
                    "kademlia routing updated"
                );
            }
            KademliaEvent::UnroutablePeer { peer } => {
                tracing::info!(?peer, "kademlia peer is unroutable");
            }
            KademliaEvent::RoutablePeer { peer, address } => {
                tracing::trace!(?peer, ?address, "kademlia peer is routable");
            }
            KademliaEvent::PendingRoutablePeer { peer, address } => {
                tracing::trace!(?peer, ?address, "kademlia peer is pending routable");
            }
            KademliaEvent::ModeChanged { new_mode } => {
                tracing::info!(?new_mode, "kademlia mode changed");
            }
        }
    }

    pub fn process_mdns_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered) => {
                for (peer_id, addr) in discovered {
                    tracing::info!(%peer_id, %addr, "peer discovered");
                }
            }
            MdnsEvent::Expired(expired) => {
                for (peer_id, addr) in expired {
                    tracing::info!(%peer_id, %addr, "peer expired");
                }
            }
        }
    }

    pub fn process_dcutr_event(&mut self, event: DcutrEvent) {
        let DcutrEvent {
            remote_peer_id,
            result,
        } = event;
        match result {
            Ok(remote_addr) => {
                tracing::info!(%remote_peer_id, %remote_addr, "dcutr success");
            }
            Err(e) => {
                tracing::error!(%remote_peer_id, %e, "dcutr failed");
            }
        }
    }

    pub fn process_relay_client_event(&mut self, event: RelayClientEvent) {
        match event {
            RelayClientEvent::ReservationReqAccepted { .. } => {}
            RelayClientEvent::OutboundCircuitEstablished { .. } => {}
            RelayClientEvent::InboundCircuitEstablished { .. } => {}
        }
    }

    pub fn process_relay_server_event(&mut self, event: RelayServerEvent) {
        match event {
            RelayServerEvent::ReservationReqAccepted { .. } => {}
            RelayServerEvent::ReservationReqDenied { .. } => {}
            RelayServerEvent::ReservationTimedOut { .. } => {}
            RelayServerEvent::CircuitReqDenied { .. } => {}
            RelayServerEvent::CircuitReqAccepted { .. } => {}
            RelayServerEvent::CircuitClosed { .. } => {}
            _ => {}
        }
    }
}

impl<C: NetworkBehaviour, T> Future for ConnexaTask<C, T>
where
    C: Send,
    C::ToSwarm: Debug,
    T: 'static,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cleanup_timer.poll_unpin(cx).is_ready() {
            let interval = self.cleanup_interval;
            self.cleanup_timer.reset(interval);
            self.gossipsub_listener.retain(|_, v| {
                v.retain(|ch| !ch.is_closed());
                !v.is_empty()
            });

            self.floodsub_listener.retain(|_, v| {
                v.retain(|ch| !ch.is_closed());
                !v.is_empty()
            });
        }

        while let Poll::Ready(Some(command)) = self.command_receiver.poll_next_unpin(cx) {
            self.process_command(command);
        }

        // Note: could probably poll in burst instead so we could continue making progress in this future
        loop {
            match self.swarm.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => self.process_swarm_event(event),
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Pending => break,
            }
        }

        while let Poll::Ready(Some((key, result))) =
            self.dht_put_record_receiver.poll_next_unpin(cx)
        {
            let record = match result {
                Ok(Ok(record)) => record,
                Ok(Err(e)) => {
                    tracing::error!(?key, ?e, "dht put record failed");
                    continue;
                }
                Err(e) => {
                    tracing::error!(?key, ?e, "dht put record failed");
                    continue;
                }
            };

            tracing::trace!(?key, ?record, "dht put record result");
            if let Some(swarm) = self.swarm.as_mut() {
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    match kad.store_mut().put(record) {
                        Ok(_) => tracing::info!(?key, "dht put record success"),
                        Err(e) => tracing::error!(?key, ?e, "dht put record failed"),
                    }
                }
            }
        }

        while let Poll::Ready(Some((key, result))) =
            self.dht_provider_record_receiver.poll_next_unpin(cx)
        {
            let record = match result {
                Ok(Ok(record)) => record,
                Ok(Err(e)) => {
                    tracing::error!(?key, ?e, "dht provider record failed");
                    continue;
                }
                Err(e) => {
                    tracing::error!(?key, ?e, "dht provider record failed");
                    continue;
                }
            };

            tracing::trace!(?key, ?record, "dht provider record result");
            if let Some(swarm) = self.swarm.as_mut() {
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    match kad.store_mut().add_provider(record) {
                        Ok(_) => tracing::info!(?key, "dht add provider record success"),
                        Err(e) => tracing::error!(?key, ?e, "dht add provider record failed"),
                    }
                }
            }
        }

        Poll::Pending
    }
}
