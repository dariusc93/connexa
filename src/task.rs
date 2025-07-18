#![allow(unused_imports)]

#[cfg(feature = "autonat")]
mod autonat;
#[cfg(feature = "dcutr")]
#[cfg(not(target_arch = "wasm32"))]
mod dcutr;
#[cfg(feature = "floodsub")]
mod floodsub;
#[cfg(feature = "gossipsub")]
mod gossipsub;
#[cfg(feature = "identify")]
mod identify;
#[cfg(feature = "kad")]
mod kad;
#[cfg(feature = "mdns")]
#[cfg(not(target_arch = "wasm32"))]
mod mdns;
#[cfg(feature = "ping")]
mod ping;
#[cfg(feature = "relay")]
mod relay;
#[cfg(feature = "rendezvous")]
mod rendezvous;
#[cfg(feature = "request-response")]
mod request_response;
#[cfg(feature = "stream")]
mod stream;
mod swarm;
#[cfg(feature = "upnp")]
#[cfg(not(target_arch = "wasm32"))]
mod upnp;

use crate::behaviour::BehaviourEvent;
use crate::types::{Command, ConnectionEvent, SwarmCommand};
use crate::{TEventCallback, TPollableCallback, TSwarmEventCallback, TTaskCallback, behaviour};

#[cfg(feature = "gossipsub")]
use crate::types::GossipsubMessage;
#[cfg(feature = "request-response")]
use crate::types::RequestResponseCommand;
#[cfg(feature = "kad")]
use crate::types::{DHTCommand, DHTEvent, RecordHandle};
#[cfg(feature = "floodsub")]
use crate::types::{FloodsubEvent, FloodsubMessage, PubsubFloodsubPublish};

#[cfg(feature = "gossipsub")]
use crate::types::GossipsubEvent;
#[cfg(feature = "stream")]
use crate::types::StreamCommand;
use crate::types::{BlacklistCommand, ConnectionLimitsCommand, WhitelistCommand};
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
#[cfg(not(target_arch = "wasm32"))]
use libp2p::mdns::Event as MdnsEvent;
#[cfg(feature = "ping")]
use libp2p::ping::Event as PingEvent;
#[cfg(feature = "relay")]
use libp2p::relay::Event as RelayServerEvent;
#[cfg(feature = "relay")]
use libp2p::relay::client::Event as RelayClientEvent;
#[cfg(feature = "rendezvous")]
use libp2p::rendezvous::server::Event as RendezvousServerEvent;
#[cfg(feature = "rendezvous")]
use libp2p::rendezvous::{Namespace, client::Event as RendezvousClientEvent};
use libp2p::swarm::derive_prelude::ListenerId;
use libp2p::swarm::{ConnectionId, NetworkBehaviour, SwarmEvent};
#[cfg(feature = "upnp")]
#[cfg(not(target_arch = "wasm32"))]
use libp2p::upnp::Event as UpnpEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use pollable_map::futures::set::FutureSet;
use pollable_map::optional::Optional;
use pollable_map::stream::StreamMap;
use std::collections::{HashMap, HashSet};
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
    pub custom_task_callback: TTaskCallback<C, X, T>,
    pub custom_event_callback: TEventCallback<C, X>,
    pub swarm_event_callback: TSwarmEventCallback<C>,
    pub custom_pollable_callback: TPollableCallback<C, X>,

    pub connection_listeners: Vec<mpsc::Sender<ConnectionEvent>>,

    pub listener_addresses: HashMap<ListenerId, Vec<Multiaddr>>,

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

    #[cfg(feature = "kad")]
    pub pending_dht_bootstrap: IndexMap<QueryId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_connection: IndexMap<ConnectionId, oneshot::Sender<std::io::Result<ConnectionId>>>,
    pub pending_disconnection_by_connection_id:
        IndexMap<ConnectionId, oneshot::Sender<std::io::Result<()>>>,
    pub pending_disconnection_by_peer_id: IndexMap<PeerId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_listen_on: IndexMap<ListenerId, oneshot::Sender<std::io::Result<ListenerId>>>,
    pub pending_remove_listener: IndexMap<ListenerId, oneshot::Sender<std::io::Result<()>>>,

    pub pending_remove_external_address: IndexMap<Multiaddr, oneshot::Sender<std::io::Result<()>>>,

    #[cfg(feature = "gossipsub")]
    pub gossipsub_listener:
        IndexMap<libp2p::gossipsub::TopicHash, Vec<mpsc::Sender<GossipsubEvent>>>,
    #[cfg(feature = "floodsub")]
    pub floodsub_listener: IndexMap<libp2p::floodsub::Topic, Vec<mpsc::Sender<FloodsubEvent>>>,

    #[cfg(feature = "rendezvous")]
    pub pending_rendezvous_register:
        IndexMap<(PeerId, Namespace), Vec<oneshot::Sender<std::io::Result<()>>>>,

    #[cfg(feature = "rendezvous")]
    pub pending_rendezvous_discover: IndexMap<
        PeerId,
        IndexMap<
            Namespace,
            Vec<
                oneshot::Sender<
                    std::io::Result<(libp2p::rendezvous::Cookie, Vec<(PeerId, Vec<Multiaddr>)>)>,
                >,
            >,
        >,
    >,

    #[cfg(feature = "rendezvous")]
    pub pending_rendezvous_discover_any: IndexMap<
        PeerId,
        Vec<
            oneshot::Sender<
                std::io::Result<(libp2p::rendezvous::Cookie, Vec<(PeerId, Vec<Multiaddr>)>)>,
            >,
        >,
    >,

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
            custom_pollable_callback: Box::new(|_, _, _| Poll::Pending),
            swarm_event_callback: Box::new(|_| ()),
            connection_listeners: Vec::new(),
            listener_addresses: HashMap::new(),
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
            #[cfg(feature = "kad")]
            pending_dht_bootstrap: Default::default(),
            cleanup_timer: Delay::new(duration),
            cleanup_interval: duration,
            pending_connection: IndexMap::new(),
            pending_disconnection_by_peer_id: IndexMap::new(),
            pending_disconnection_by_connection_id: IndexMap::new(),
            pending_listen_on: IndexMap::new(),
            pending_remove_listener: IndexMap::new(),
            pending_remove_external_address: IndexMap::new(),
            #[cfg(feature = "floodsub")]
            floodsub_listener: Default::default(),
            #[cfg(feature = "gossipsub")]
            gossipsub_listener: Default::default(),
            #[cfg(feature = "rendezvous")]
            pending_rendezvous_discover: Default::default(),
            #[cfg(feature = "rendezvous")]
            pending_rendezvous_register: Default::default(),
            #[cfg(feature = "rendezvous")]
            pending_rendezvous_discover_any: Default::default(),
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

    pub fn set_pollable_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut Context<'_>, &mut Swarm<behaviour::Behaviour<C>>, &mut X) -> Poll<()>
            + Send
            + 'static,
    {
        self.custom_pollable_callback = Box::new(callback);
    }

    pub fn process_command(&mut self, command: Command<T>) {
        let Some(swarm) = self.swarm.as_mut() else {
            return;
        };
        match command {
            Command::Swarm(swarm_command) => match swarm_command {
                SwarmCommand::Listener { resp } => {
                    let (tx, rx) = mpsc::channel(50);
                    self.connection_listeners.push(tx);
                    let _ = resp.send(rx);
                }
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
                SwarmCommand::GetListeningAddress { id, resp } => {
                    let Some(addrs) = self.listener_addresses.get(&id) else {
                        let _ = resp.send(Err(std::io::Error::other("listener not found")));
                        return;
                    };

                    let _ = resp.send(Ok(addrs.clone()));
                }
                SwarmCommand::RemoveListener { listener_id, resp } => {
                    if !swarm.remove_listener(listener_id) {
                        let _ = resp.send(Err(std::io::Error::other("listener not found")));
                        return;
                    }
                    self.listener_addresses.remove(&listener_id);
                    self.pending_remove_listener.insert(listener_id, resp);
                }
                SwarmCommand::AddExternalAddress { address, resp } => {
                    swarm.add_external_address(address);
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
                    swarm.add_peer_address(peer_id, address);
                    let _ = resp.send(Ok(()));
                }
            },
            Command::Whitelist(command) => match command {
                WhitelistCommand::Add { peer_id, resp } => {
                    let Some(whitelist) = swarm.behaviour_mut().allow_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("whitelist not enabled")));
                        return;
                    };

                    if !whitelist.allow_peer(peer_id) {
                        let _ =
                            resp.send(Err(std::io::Error::other("peer is already whitelisted")));
                        return;
                    }

                    let _ = resp.send(Ok(()));
                }
                WhitelistCommand::Remove { peer_id, resp } => {
                    let Some(whitelist) = swarm.behaviour_mut().allow_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("whitelist not enabled")));
                        return;
                    };

                    if !whitelist.disallow_peer(peer_id) {
                        let _ = resp.send(Err(std::io::Error::other("peer is not whitelisted")));
                        return;
                    }

                    let _ = resp.send(Ok(()));
                }
                WhitelistCommand::List { resp } => {
                    let Some(whitelist) = swarm.behaviour_mut().allow_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("whitelist not enabled")));
                        return;
                    };

                    let list = whitelist.allowed_peers();
                    let list = list.iter().cloned().collect();

                    let _ = resp.send(Ok(list));
                }
            },
            Command::Blacklist(command) => match command {
                BlacklistCommand::Add { peer_id, resp } => {
                    let Some(blacklist) = swarm.behaviour_mut().deny_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("blacklist not enabled")));
                        return;
                    };

                    if !blacklist.block_peer(peer_id) {
                        let _ =
                            resp.send(Err(std::io::Error::other("peer is already blacklisted")));
                        return;
                    }

                    let _ = resp.send(Ok(()));
                }
                BlacklistCommand::Remove { peer_id, resp } => {
                    let Some(blacklist) = swarm.behaviour_mut().deny_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("blacklist not enabled")));
                        return;
                    };

                    if !blacklist.unblock_peer(peer_id) {
                        let _ = resp.send(Err(std::io::Error::other("peer is not blacklisted")));
                        return;
                    }

                    let _ = resp.send(Ok(()));
                }
                BlacklistCommand::List { resp } => {
                    let Some(blacklist) = swarm.behaviour_mut().deny_list.as_mut() else {
                        let _ = resp.send(Err(std::io::Error::other("blacklist not enabled")));
                        return;
                    };

                    let list = blacklist.blocked_peers();
                    let list = list.iter().cloned().collect();

                    let _ = resp.send(Ok(list));
                }
            },
            Command::ConnectionLimits(command) => match command {
                ConnectionLimitsCommand::Get { resp } => {
                    let Some(connection_limits) = swarm.behaviour_mut().connection_limits.as_mut()
                    else {
                        let _ =
                            resp.send(Err(std::io::Error::other("connection limits not enabled")));
                        return;
                    };

                    let limits = connection_limits.limits_mut();
                    let _ = resp.send(Ok(limits.clone()));
                }
                ConnectionLimitsCommand::Set { limits, resp } => {
                    let Some(connection_limits) = swarm.behaviour_mut().connection_limits.as_mut()
                    else {
                        let _ =
                            resp.send(Err(std::io::Error::other("connection limits not enabled")));
                        return;
                    };

                    *connection_limits.limits_mut() = limits;

                    let _ = resp.send(Ok(()));
                }
            },
            #[cfg(feature = "gossipsub")]
            Command::Gossipsub(command) => self.process_gossipsub_command(command),
            #[cfg(feature = "floodsub")]
            Command::Floodsub(command) => self.process_floodsub_command(command),
            #[cfg(feature = "autonat")]
            Command::Autonat(autonat_command) => self.process_autonat_v1_command(autonat_command),
            #[cfg(feature = "kad")]
            Command::Dht(dht_command) => self.process_kademlia_command(dht_command),
            #[cfg(feature = "stream")]
            Command::Stream(stream_command) => self.process_stream_command(stream_command),
            #[cfg(feature = "request-response")]
            Command::RequestResponse(request_response_command) => {
                self.process_request_response_command(request_response_command)
            }
            #[cfg(feature = "rendezvous")]
            Command::Rendezvous(rendezvous_command) => {
                self.process_rendezvous_command(rendezvous_command)
            }
            Command::Custom(custom_command) => {
                (self.custom_task_callback)(swarm, &mut self.context, custom_command);
            }
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
        // We should terminate the future if swarm stream is done being polled
        if self.swarm.is_none() {
            return Poll::Ready(());
        }

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

        {
            let this = &mut *self;
            if let Some(swarm) = this.swarm.as_mut() {
                let _ = (this.custom_pollable_callback)(cx, swarm, &mut this.context);
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
