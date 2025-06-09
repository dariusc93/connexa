use crate::prelude::{DHTEvent, NetworkBehaviour, RecordHandle};
use crate::task::ConnexaTask;
use futures::channel::oneshot;
use libp2p::kad::{
    AddProviderOk, BootstrapError, BootstrapOk, Event as KademliaEvent, GetClosestPeersOk,
    GetProvidersOk, GetRecordOk, InboundRequest, PutRecordOk, QueryResult,
};
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
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
}
