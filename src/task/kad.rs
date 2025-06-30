use crate::prelude::{DHTEvent, RecordHandle};
use crate::task::ConnexaTask;
use crate::types::DHTCommand;
use futures::channel::{mpsc, oneshot};
use libp2p::kad::{
    AddProviderOk, BootstrapError, BootstrapOk, Event as KademliaEvent, GetClosestPeersOk,
    GetProvidersOk, GetRecordOk, InboundRequest, PutRecordOk, QueryResult, Record, RoutingUpdate,
};
use std::fmt::Debug;
use std::io;

use libp2p::swarm::NetworkBehaviour;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_kademlia_command(&mut self, command: DHTCommand) {
        let swarm = self.swarm.as_mut().unwrap();
        match command {
            DHTCommand::FindPeer { peer_id, resp } => {
                let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                    return;
                };

                let id = kad.get_closest_peers(peer_id);

                self.pending_dht_find_closest_peer.insert(id, resp);
            }
            DHTCommand::Bootstrap { lazy, resp } => {
                let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("kademlia is not enabled")));
                    return;
                };

                let id = match kad.bootstrap() {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = resp.send(Err(std::io::Error::other(e)));
                        return;
                    }
                };

                if lazy {
                    let _ = resp.send(Ok(()));
                    return;
                }

                self.pending_dht_bootstrap.insert(id, resp);
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
                                self.dht_put_record_global_receiver.insert(rx);
                                event.set_record_confirmation(tx)
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
                step,
            } => match result {
                QueryResult::Bootstrap(result) => match result {
                    Ok(BootstrapOk {
                        peer,
                        num_remaining,
                    }) => {
                        tracing::info!(?peer, ?num_remaining, "kademlia bootstrap");
                        if step.last {
                            if let Some(ch) = self.pending_dht_bootstrap.shift_remove(&id) {
                                let _ = ch.send(Ok(()));
                            }
                        }
                    }
                    Err(
                        e @ BootstrapError::Timeout {
                            peer,
                            num_remaining,
                        },
                    ) => {
                        tracing::info!(?peer, ?num_remaining, "kademlia bootstrap timeout");
                        if let Some(ch) = self.pending_dht_bootstrap.shift_remove(&id) {
                            let _ = ch.send(Err(io::Error::new(io::ErrorKind::TimedOut, e)));
                        }
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
}
