#![allow(unused_imports)]
use crate::behaviour;
use crate::behaviour::BehaviourEvent;
use crate::types::{Command, SwarmCommand};

#[cfg(feature = "gossipsub")]
use crate::types::GossipsubMessage;
#[cfg(feature = "request-response")]
use crate::types::RequestResponseCommand;
#[cfg(feature = "kad")]
use crate::types::{DHTCommand, DHTEvent, RecordHandle};
#[cfg(feature = "floodsub")]
use crate::types::{FloodsubMessage, PubsubFloodsubPublish};
#[cfg(any(feature = "floodsub", feature = "gossipsub"))]
use crate::types::{PubsubCommand, PubsubEvent, PubsubPublishType, PubsubType};

#[cfg(feature = "stream")]
use crate::types::StreamCommand;
use either::Either;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, StreamExt};
use futures_timer::Delay;
use indexmap::IndexMap;
#[cfg(feature = "autonat")]
use libp2p::autonat::v1::Event as AutonatV1Event;
#[cfg(feature = "autonat")]
use libp2p::autonat::v2::client::Event as AutonatV2ClientEvent;
#[cfg(feature = "autonat")]
use libp2p::autonat::v2::server::Event as AutonatV2ServerEvent;
#[cfg(all(feature = "relay", feature = "dcutr"))]
use libp2p::dcutr::Event as DcutrEvent;
#[cfg(feature = "floodsub")]
use libp2p::floodsub::FloodsubEvent;
#[cfg(feature = "gossipsub")]
use libp2p::gossipsub::Event as GossipsubEvent;
#[cfg(feature = "identify")]
use libp2p::identify::Event as IdentifyEvent;
#[cfg(feature = "kad")]
use libp2p::kad::store::RecordStore;
#[cfg(feature = "kad")]
use libp2p::kad::{
    AddProviderOk, BootstrapError, BootstrapOk, Event as KademliaEvent, GetClosestPeersOk,
    GetProvidersOk, GetRecordOk, InboundRequest, PeerInfo, PeerRecord, ProviderRecord, PutRecordOk,
    QueryId, QueryResult, Record, RecordKey as Key, RoutingUpdate,
};
#[cfg(feature = "mdns")]
use libp2p::mdns::Event as MdnsEvent;
#[cfg(feature = "ping")]
use libp2p::ping::Event as PingEvent;
#[cfg(feature = "relay")]
use libp2p::relay::Event as RelayServerEvent;
#[cfg(feature = "relay")]
use libp2p::relay::client::Event as RelayClientEvent;
#[cfg(feature = "rendezvous")]
use libp2p::rendezvous::client::Event as RendezvousClientEvent;
#[cfg(feature = "rendezvous")]
use libp2p::rendezvous::server::Event as RendezvousServerEvent;
use libp2p::swarm::derive_prelude::ListenerId;
use libp2p::swarm::{ConnectionId, NetworkBehaviour, SwarmEvent};
#[cfg(feature = "upnp")]
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

pub struct ConnexaTask<X, C: NetworkBehaviour, T = ()>
where
    C: Send,
    C::ToSwarm: Debug,
{
    pub swarm: Optional<Swarm<behaviour::Behaviour<C>>>,
    pub command_receiver: Optional<mpsc::Receiver<Command<T>>>,
    pub context: X,
    pub custom_task_callback:
        Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, T) + 'static + Send>,
    pub custom_event_callback:
        Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, C::ToSwarm) + 'static + Send>,
    pub swarm_event_callback: Box<dyn Fn(&SwarmEvent<BehaviourEvent<C>>) + 'static + Send>,

    /// A listener for sending dht events
    #[cfg(feature = "kad")]
    pub dht_event_sender: IndexMap<Key, Vec<mpsc::Sender<DHTEvent>>>,
    #[cfg(feature = "kad")]
    pub dht_event_global_sender: Vec<mpsc::Sender<DHTEvent>>,

    #[cfg(feature = "kad")]
    pub dht_put_record_receiver:
        StreamMap<Key, FutureSet<oneshot::Receiver<std::io::Result<Record>>>>,
    #[cfg(feature = "kad")]
    pub dht_put_record_global_receiver: FutureSet<oneshot::Receiver<std::io::Result<Record>>>,
    #[cfg(feature = "kad")]
    pub dht_provider_record_receiver:
        StreamMap<Key, FutureSet<oneshot::Receiver<std::io::Result<ProviderRecord>>>>,
    #[cfg(feature = "kad")]
    pub dht_provider_record_global_receiver:
        FutureSet<oneshot::Receiver<std::io::Result<ProviderRecord>>>,
    #[cfg(feature = "kad")]
    pub pending_dht_put_record: IndexMap<QueryId, oneshot::Sender<std::io::Result<()>>>,
    #[cfg(feature = "kad")]
    pub pending_dht_put_provider_record: IndexMap<QueryId, oneshot::Sender<std::io::Result<()>>>,
    #[cfg(feature = "kad")]
    pub pending_dht_get_record: IndexMap<QueryId, mpsc::Sender<std::io::Result<PeerRecord>>>,
    #[cfg(feature = "kad")]
    pub pending_dht_get_provider_record:
        IndexMap<QueryId, mpsc::Sender<std::io::Result<HashSet<PeerId>>>>,
    #[cfg(feature = "kad")]
    pub pending_dht_find_closest_peer:
        IndexMap<QueryId, oneshot::Sender<std::io::Result<Vec<PeerInfo>>>>,

    pub pending_connection: IndexMap<ConnectionId, oneshot::Sender<std::io::Result<ConnectionId>>>,
    pub pending_disconnection_by_connection_id:
        IndexMap<ConnectionId, oneshot::Sender<std::io::Result<()>>>,
    pub pending_disconnection_by_peer_id: IndexMap<PeerId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_listen_on: IndexMap<ListenerId, oneshot::Sender<std::io::Result<ListenerId>>>,
    pub pending_remove_listener: IndexMap<ListenerId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_remove_external_address: IndexMap<Multiaddr, oneshot::Sender<std::io::Result<()>>>,

    pub pending_add_peer_address:
        IndexMap<(PeerId, Multiaddr), oneshot::Sender<std::io::Result<()>>>,

    #[cfg(feature = "gossipsub")]
    pub gossipsub_listener:
        IndexMap<libp2p::gossipsub::TopicHash, Vec<mpsc::Sender<PubsubEvent<GossipsubMessage>>>>,
    #[cfg(feature = "floodsub")]
    pub floodsub_listener:
        IndexMap<libp2p::floodsub::Topic, Vec<mpsc::Sender<PubsubEvent<FloodsubMessage>>>>,

    pub cleanup_timer: Delay,
    pub cleanup_interval: Duration,
}

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn new(swarm: Swarm<behaviour::Behaviour<C>>) -> Self {
        let duration = Duration::from_secs(10);
        Self {
            swarm: Optional::new(swarm),
            context: X::default(),
            command_receiver: Optional::default(),
            custom_event_callback: Box::new(|_, _, _| ()),
            custom_task_callback: Box::new(|_, _, _| ()),
            swarm_event_callback: Box::new(|_| ()),
            #[cfg(feature = "kad")]
            dht_event_sender: Default::default(),
            #[cfg(feature = "kad")]
            dht_event_global_sender: vec![],
            #[cfg(feature = "kad")]
            dht_put_record_receiver: StreamMap::new(),
            #[cfg(feature = "kad")]
            dht_put_record_global_receiver: Default::default(),
            #[cfg(feature = "kad")]
            dht_provider_record_receiver: StreamMap::new(),
            #[cfg(feature = "kad")]
            dht_provider_record_global_receiver: Default::default(),
            #[cfg(feature = "kad")]
            pending_dht_put_record: Default::default(),
            #[cfg(feature = "kad")]
            pending_dht_put_provider_record: IndexMap::new(),
            #[cfg(feature = "kad")]
            pending_dht_get_record: Default::default(),
            #[cfg(feature = "kad")]
            pending_dht_get_provider_record: Default::default(),
            #[cfg(feature = "kad")]
            pending_dht_find_closest_peer: Default::default(),
            cleanup_timer: Delay::new(duration),
            cleanup_interval: duration,
            pending_connection: IndexMap::new(),
            pending_disconnection_by_peer_id: IndexMap::new(),
            pending_disconnection_by_connection_id: IndexMap::new(),
            pending_listen_on: IndexMap::new(),
            pending_remove_listener: IndexMap::new(),
            pending_remove_external_address: IndexMap::new(),
            pending_add_peer_address: IndexMap::new(),
            #[cfg(feature = "floodsub")]
            floodsub_listener: Default::default(),
            #[cfg(feature = "gossipsub")]
            gossipsub_listener: Default::default(),
        }
    }

    pub fn set_context(&mut self, context: X) {
        self.context = context;
    }

    pub fn set_command_receiver(&mut self, command_receiver: mpsc::Receiver<Command<T>>) {
        self.command_receiver.replace(command_receiver);
    }

    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, C::ToSwarm) + Send + 'static,
    {
        self.custom_event_callback = Box::new(callback);
    }

    pub fn set_task_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, T) + Send + 'static,
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
            #[cfg(any(feature = "gossipsub", feature = "floodsub"))]
            Command::Pubsub(pubsub_command) => match pubsub_command {
                #[cfg(feature = "gossipsub")]
                PubsubCommand::Subscribe {
                    pubsub_type: PubsubType::Gossipsub,
                    topic,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic);
                    match pubsub.subscribe(&topic) {
                        Ok(true) => {
                            let _ = resp.send(Ok(()));
                        }
                        Ok(false) => {
                            let _ =
                                resp.send(Err(std::io::Error::other("topic already subscribed")));
                        }
                        Err(e) => {
                            let _ = resp.send(Err(std::io::Error::other(e)));
                        }
                    }
                }
                #[cfg(feature = "floodsub")]
                PubsubCommand::Subscribe {
                    pubsub_type: PubsubType::Floodsub,
                    topic,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::floodsub::Topic::new(topic);
                    match pubsub.subscribe(topic) {
                        true => {
                            let _ = resp.send(Ok(()));
                        }
                        false => {
                            let _ =
                                resp.send(Err(std::io::Error::other("topic already subscribed")));
                        }
                    }
                }
                #[cfg(feature = "floodsub")]
                PubsubCommand::Unsubscribe {
                    pubsub_type: PubsubType::Floodsub,
                    topic,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::floodsub::Topic::new(topic);
                    match pubsub.unsubscribe(topic) {
                        true => {
                            let _ = resp.send(Ok(()));
                        }
                        false => {
                            let _ =
                                resp.send(Err(std::io::Error::other("not subscribed to topic")));
                        }
                    }
                }
                #[cfg(feature = "gossipsub")]
                PubsubCommand::Unsubscribe {
                    pubsub_type: PubsubType::Gossipsub,
                    topic,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic);
                    match pubsub.unsubscribe(&topic) {
                        true => {
                            let _ = resp.send(Ok(()));
                        }
                        false => {
                            let _ =
                                resp.send(Err(std::io::Error::other("not subscribed to topic")));
                        }
                    }
                }
                #[cfg(feature = "floodsub")]
                PubsubCommand::Subscribed {
                    pubsub_type: PubsubType::Floodsub,
                    resp,
                } => {
                    if !swarm.behaviour_mut().floodsub.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                        return;
                    }

                    let _ = resp.send(Err(std::io::Error::other(
                        "function is unimplemented at this time",
                    )));
                }
                #[cfg(feature = "gossipsub")]
                PubsubCommand::Subscribed {
                    pubsub_type: PubsubType::Gossipsub,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let topics = pubsub.topics().map(|topic| topic.to_string()).collect();

                    let _ = resp.send(Ok(topics));
                }
                #[cfg(feature = "gossipsub")]
                PubsubCommand::Peers {
                    pubsub_type: PubsubType::Gossipsub,
                    topic,
                    resp,
                } => {
                    let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic).hash();
                    let peers = pubsub.mesh_peers(&topic).copied().collect();

                    let _ = resp.send(Ok(peers));
                }
                #[cfg(feature = "floodsub")]
                PubsubCommand::Peers {
                    pubsub_type: PubsubType::Floodsub,
                    resp,
                    ..
                } => {
                    if !swarm.behaviour_mut().floodsub.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let _ = resp.send(Err(std::io::Error::other(
                        "function is unimplemented at this time",
                    )));
                }
                #[cfg(feature = "gossipsub")]
                PubsubCommand::Publish(PubsubPublishType::Gossipsub { topic, data, resp }) => {
                    let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    };

                    let topic = libp2p::gossipsub::IdentTopic::new(topic);
                    let ret = match pubsub.publish(topic, data) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(std::io::Error::other(e)),
                    };

                    let _ = resp.send(ret);
                }
                #[cfg(feature = "floodsub")]
                PubsubCommand::Publish(PubsubPublishType::Floodsub(pubsub_type, resp)) => {
                    let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                        return;
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
                #[cfg(feature = "floodsub")]
                PubsubCommand::FloodsubListener { topic, resp } => {
                    if !swarm.behaviour_mut().floodsub.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                        return;
                    }

                    let topic = libp2p::floodsub::Topic::new(topic);

                    let (tx, rx) = mpsc::channel(50);

                    self.floodsub_listener.entry(topic).or_default().push(tx);

                    let _ = resp.send(Ok(rx));
                }
                #[cfg(feature = "gossipsub")]
                PubsubCommand::GossipsubListener { topic, resp } => {
                    if !swarm.behaviour_mut().gossipsub.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                        return;
                    }

                    let topic = libp2p::gossipsub::IdentTopic::new(topic).hash();

                    let (tx, rx) = mpsc::channel(50);

                    self.gossipsub_listener.entry(topic).or_default().push(tx);

                    let _ = resp.send(Ok(rx));
                }
            },
            #[cfg(feature = "kad")]
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
                DHTCommand::Listener {
                    key: Some(key),
                    resp,
                } => {
                    if !swarm.behaviour_mut().kademlia.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    }

                    let (tx, rx) = mpsc::channel(10);

                    self.dht_event_sender.entry(key).or_default().push(tx);

                    let _ = resp.send(Ok(rx));
                }

                DHTCommand::Listener { key: _, resp } => {
                    if !swarm.behaviour_mut().kademlia.is_enabled() {
                        let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                        return;
                    }

                    let (tx, rx) = mpsc::channel(10);

                    self.dht_event_global_sender.push(tx);

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

                    let id = kad.get_record(key);

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
            #[cfg(feature = "rendezvous")]
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
            #[cfg(feature = "rendezvous")]
            Command::Rendezvous(rendezvous_command) => {
                // TODO
                let _ = rendezvous_command;
            }
            Command::Custom(custom_command) => {
                (self.custom_task_callback)(swarm, &mut self.context, custom_command);
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
            #[cfg(feature = "relay")]
            BehaviourEvent::Relay(event) => self.process_relay_server_event(event),
            #[cfg(feature = "relay")]
            BehaviourEvent::RelayClient(event) => self.process_relay_client_event(event),
            #[cfg(feature = "upnp")]
            BehaviourEvent::Upnp(event) => self.process_upnp_event(event),
            #[cfg(all(feature = "dcutr", feature = "relay"))]
            BehaviourEvent::Dcutr(event) => self.process_dcutr_event(event),
            #[cfg(feature = "rendezvous")]
            BehaviourEvent::RendezvousClient(event) => self.process_rendezvous_client_event(event),
            #[cfg(feature = "rendezvous")]
            BehaviourEvent::RendezvousServer(event) => self.process_rendezvous_server_event(event),
            #[cfg(feature = "mdns")]
            BehaviourEvent::Mdns(event) => self.process_mdns_event(event),
            #[cfg(feature = "gossipsub")]
            BehaviourEvent::Gossipsub(ev) => self.process_gossipsub_event(ev),
            #[cfg(feature = "floodsub")]
            BehaviourEvent::Floodsub(ev) => self.process_floodsub_event(ev),
            #[cfg(feature = "kad")]
            BehaviourEvent::Kademlia(event) => self.process_kademlia_event(event),
            #[cfg(feature = "identify")]
            BehaviourEvent::Identify(event) => self.process_identify_event(event),
            #[cfg(feature = "ping")]
            BehaviourEvent::Ping(event) => self.process_ping_event(event),
            #[cfg(feature = "autonat")]
            BehaviourEvent::AutonatV1(event) => self.process_autonat_v1_event(event),
            #[cfg(feature = "autonat")]
            BehaviourEvent::AutonatV2Client(event) => self.process_autonat_v2_client_event(event),
            #[cfg(feature = "autonat")]
            BehaviourEvent::AutonatV2Server(event) => self.process_autonat_v2_server_event(event),
            BehaviourEvent::Custom(custom_event) => {
                (self.custom_event_callback)(swarm, &mut self.context, custom_event)
            }
            _ => unreachable!(),
        }
    }

    #[cfg(feature = "rendezvous")]
    pub fn process_rendezvous_client_event(&mut self, event: RendezvousClientEvent) {
        match event {
            RendezvousClientEvent::Discovered { .. } => {}
            RendezvousClientEvent::DiscoverFailed { .. } => {}
            RendezvousClientEvent::Registered { .. } => {}
            RendezvousClientEvent::RegisterFailed { .. } => {}
            RendezvousClientEvent::Expired { .. } => {}
        }
    }

    #[cfg(feature = "rendezvous")]
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

    #[cfg(feature = "upnp")]
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

    #[cfg(feature = "gossipsub")]
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

    #[cfg(feature = "floodsub")]
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

    #[cfg(feature = "autonat")]
    pub fn process_autonat_v1_event(&mut self, event: AutonatV1Event) {
        match event {
            AutonatV1Event::InboundProbe(_) => {}
            AutonatV1Event::OutboundProbe(_) => {}
            AutonatV1Event::StatusChanged { old, new } => {
                tracing::info!(old = ?old, new = ?new, "nat status changed");
            }
        }
    }

    #[cfg(feature = "autonat")]
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

    #[cfg(feature = "autonat")]
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

    #[cfg(feature = "ping")]
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

    #[cfg(feature = "identify")]
    pub fn process_identify_event(&mut self, event: IdentifyEvent) {
        let Some(swarm) = self.swarm.as_mut() else {
            return;
        };
        match event {
            IdentifyEvent::Received {
                peer_id,
                connection_id,
                info,
            } => {
                tracing::info!(%peer_id, %connection_id, ?info, "identify received");
                let libp2p::identify::Info {
                    listen_addrs,
                    protocols,
                    ..
                } = info;

                #[cfg(feature = "identify")]
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    if protocols.iter().any(|p| libp2p::kad::PROTOCOL_NAME.eq(p)) {
                        for addr in listen_addrs {
                            kad.add_address(&peer_id, addr.clone());
                        }
                    }
                }

                let _ = listen_addrs;
                let _ = protocols;
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

    #[cfg(feature = "kad")]
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
                    let event = DHTEvent::ProvideRecord {
                        record: RecordHandle {
                            record: record.clone(),
                            confirm: None,
                        },
                    };
                    for ch_sender in self.dht_event_global_sender.iter_mut() {
                        let event = match record.is_some() {
                            true => {
                                let (tx, rx) = oneshot::channel();
                                self.dht_provider_record_global_receiver.insert(rx);
                                event.set_provider_confirmation(tx)
                            }
                            false => event.clone(),
                        };
                        let _ = ch_sender.try_send(event);
                    }

                    let Some(record) = record else {
                        return;
                    };

                    tracing::trace!(?record, "kademlia add provider request");

                    let key = record.key.clone();

                    let Some(sender) = self.dht_event_sender.get_mut(&key) else {
                        return;
                    };

                    for ch_sender in sender.iter_mut() {
                        let (tx, rx) = oneshot::channel();
                        let _ = ch_sender.try_send(event.set_provider_confirmation(tx));
                        let set = self.dht_provider_record_receiver.get_mut_or_default(&key);
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

                    let event = DHTEvent::PutRecord {
                        source,
                        record: RecordHandle {
                            record: record.clone(),
                            confirm: None,
                        },
                    };
                    for ch_sender in self.dht_event_global_sender.iter_mut() {
                        let event = match record.is_some() {
                            true => {
                                let (tx, rx) = oneshot::channel();
                                self.dht_provider_record_global_receiver.insert(rx);
                                event.set_provider_confirmation(tx)
                            }
                            false => event.clone(),
                        };
                        let _ = ch_sender.try_send(event);
                    }

                    let Some(record) = record else {
                        return;
                    };

                    let key = record.key.clone();
                    let Some(sender) = self.dht_event_sender.get_mut(&key) else {
                        return;
                    };

                    for ch_sender in sender.iter_mut() {
                        let (tx, rx) = oneshot::channel();
                        let _ = ch_sender.try_send(event.set_record_confirmation(tx));

                        let set = self.dht_put_record_receiver.get_mut_or_default(&key);

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
                        if let Some(ch) = self.pending_dht_find_closest_peer.shift_remove(&id) {
                            let _ = ch.send(Ok(peers));
                        }
                    }
                    Err(e) => {
                        tracing::error!(%id, %e, "kademlia get closest peers error");
                        if let Some(ch) = self.pending_dht_find_closest_peer.shift_remove(&id) {
                            let _ =
                                ch.send(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, e)));
                        }
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

    #[cfg(feature = "mdns")]
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

    #[cfg(all(feature = "relay", feature = "dcutr"))]
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

    #[cfg(feature = "relay")]
    pub fn process_relay_client_event(&mut self, event: RelayClientEvent) {
        match event {
            RelayClientEvent::ReservationReqAccepted { .. } => {}
            RelayClientEvent::OutboundCircuitEstablished { .. } => {}
            RelayClientEvent::InboundCircuitEstablished { .. } => {}
        }
    }

    #[cfg(feature = "relay")]
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

impl<X, C: NetworkBehaviour, T> Future for ConnexaTask<X, C, T>
where
    X: Default + Unpin + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
    T: 'static,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.cleanup_timer.poll_unpin(cx).is_ready() {
            let interval = self.cleanup_interval;
            self.cleanup_timer.reset(interval);
            #[cfg(feature = "gossipsub")]
            self.gossipsub_listener.retain(|_, v| {
                v.retain(|ch| !ch.is_closed());
                !v.is_empty()
            });

            #[cfg(feature = "floodsub")]
            self.floodsub_listener.retain(|_, v| {
                v.retain(|ch| !ch.is_closed());
                !v.is_empty()
            });

            #[cfg(feature = "kad")]
            self.dht_event_sender.retain(|_, v| {
                v.retain(|ch| !ch.is_closed());
                !v.is_empty()
            });

            #[cfg(feature = "kad")]
            self.dht_event_global_sender.retain(|ch| !ch.is_closed());
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

        #[cfg(feature = "kad")]
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

        #[cfg(feature = "kad")]
        while let Poll::Ready(Some(result)) =
            self.dht_put_record_global_receiver.poll_next_unpin(cx)
        {
            let record = match result {
                Ok(Ok(record)) => record,
                Ok(Err(e)) => {
                    tracing::error!(?e, "dht put record failed");
                    continue;
                }
                Err(e) => {
                    tracing::error!(?e, "dht put record failed");
                    continue;
                }
            };

            let key = record.key.clone();
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

        #[cfg(feature = "kad")]
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

        #[cfg(feature = "kad")]
        while let Poll::Ready(Some(result)) =
            self.dht_provider_record_global_receiver.poll_next_unpin(cx)
        {
            let record = match result {
                Ok(Ok(record)) => record,
                Ok(Err(e)) => {
                    tracing::error!(?e, "dht provider record failed");
                    continue;
                }
                Err(e) => {
                    tracing::error!(?e, "dht provider record failed");
                    continue;
                }
            };

            let key = record.key.clone();

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
