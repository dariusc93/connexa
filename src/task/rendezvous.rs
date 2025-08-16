use crate::behaviour::peer_store::store::Store;
use crate::task::ConnexaTask;
use crate::types::RendezvousCommand;
use futures::SinkExt;
use libp2p::rendezvous::client::Event as RendezvousClientEvent;
use libp2p::rendezvous::server::Event as RendezvousServerEvent;
use libp2p::rendezvous::{Namespace, Registration};
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, S, T> ConnexaTask<X, C, S, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
    S: Store,
{
    pub fn process_rendezvous_command(&mut self, command: RendezvousCommand) {
        let swarm = self.swarm.as_mut().expect("swarm is active");
        match command {
            RendezvousCommand::Register {
                namespace,
                peer_id,
                ttl,
                resp,
            } => {
                let Some(rz) = swarm.behaviour_mut().rendezvous_client.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "rendezvous client is not enabled",
                    )));
                    return;
                };

                if let Err(e) = rz.register(namespace.clone(), peer_id, ttl) {
                    let _ = resp.send(Err(std::io::Error::other(e)));
                    return;
                }

                self.pending_rendezvous_register
                    .entry((peer_id, namespace))
                    .or_default()
                    .push(resp);
            }
            RendezvousCommand::Unregister {
                namespace,
                peer_id,
                resp,
            } => {
                let Some(rz) = swarm.behaviour_mut().rendezvous_client.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "rendezvous client is not enabled",
                    )));
                    return;
                };

                rz.unregister(namespace.clone(), peer_id);

                let _ = resp.send(Ok(()));
            }
            RendezvousCommand::Discover {
                namespace: ns,
                peer_id,
                cookie,
                ttl,
                resp,
            } => {
                let Some(rz) = swarm.behaviour_mut().rendezvous_client.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "rendezvous client is not enabled",
                    )));
                    return;
                };

                rz.discover(ns.clone(), cookie, ttl, peer_id);

                match ns {
                    Some(ns) => {
                        let namespaces =
                            self.pending_rendezvous_discover.entry(peer_id).or_default();
                        namespaces.entry(ns).or_default().push(resp);
                    }
                    None => {
                        self.pending_rendezvous_discover_any
                            .entry(peer_id)
                            .or_default()
                            .push(resp);
                    }
                }
            }
        }
    }

    pub fn process_rendezvous_server_event(&mut self, event: RendezvousServerEvent) {
        match event {
            RendezvousServerEvent::DiscoverServed {
                enquirer,
                registrations,
            } => {
                tracing::debug!(%enquirer, ?registrations, "discovered");
            }
            RendezvousServerEvent::DiscoverNotServed { enquirer, error } => {
                tracing::error!(%enquirer, ?error, "failed to serve a discover request");
            }
            RendezvousServerEvent::PeerRegistered { peer, registration } => {
                let namespace = registration.namespace;
                tracing::debug!(%peer, %namespace, "registered to namespace");
            }
            RendezvousServerEvent::PeerNotRegistered {
                peer,
                namespace,
                error,
            } => {
                tracing::error!(%peer, %namespace, ?error, "not register to namespace");
            }
            RendezvousServerEvent::PeerUnregistered { peer, namespace } => {
                tracing::debug!(%peer, %namespace, "unregistered from namespace");
            }
            RendezvousServerEvent::RegistrationExpired(Registration {
                namespace,
                record,
                ttl,
            }) => {
                let peer_id = record.peer_id();
                tracing::debug!(%namespace, %peer_id, %ttl, "peer registration expired");
            }
        }
    }

    pub fn process_rendezvous_client_event(&mut self, event: RendezvousClientEvent) {
        match event {
            RendezvousClientEvent::Discovered {
                rendezvous_node,
                registrations,
                cookie,
            } => {
                tracing::debug!(%rendezvous_node, ?cookie, ?registrations, "discovered");

                let discovered_peers = registrations
                    .into_iter()
                    .map(|registration| {
                        let peer_id = registration.record.peer_id();
                        let addrs = registration.record.addresses().to_vec();
                        (peer_id, addrs)
                    })
                    .collect::<Vec<_>>();

                match cookie.namespace() {
                    Some(ns) => {
                        if let Some(namespaces) =
                            self.pending_rendezvous_discover.get_mut(&rendezvous_node)
                        {
                            if let Some(list) = namespaces.shift_remove(ns) {
                                for ch in list {
                                    let _ = ch.send(Ok((cookie.clone(), discovered_peers.clone())));
                                }
                            }

                            if namespaces.is_empty() {
                                self.pending_rendezvous_discover
                                    .shift_remove(&rendezvous_node);
                            }
                        }
                    }
                    None => {
                        if let Some(list) = self
                            .pending_rendezvous_discover_any
                            .shift_remove(&rendezvous_node)
                        {
                            for ch in list {
                                let _ = ch.send(Ok((cookie.clone(), discovered_peers.clone())));
                            }
                        }
                    }
                }
            }
            RendezvousClientEvent::DiscoverFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                tracing::error!(%rendezvous_node, ?namespace, ?error, "failed to discover ");
                match namespace {
                    Some(ns) => {
                        if let Some(namespaces) =
                            self.pending_rendezvous_discover.get_mut(&rendezvous_node)
                        {
                            if let Some(list) = namespaces.shift_remove(&ns) {
                                for ch in list {
                                    let _ = ch.send(Err(std::io::Error::other(
                                        crate::error::rendezvous::Error::from(error),
                                    )));
                                }
                            }

                            if namespaces.is_empty() {
                                self.pending_rendezvous_discover
                                    .shift_remove(&rendezvous_node);
                            }
                        }
                    }
                    None => {
                        if let Some(list) = self
                            .pending_rendezvous_discover_any
                            .shift_remove(&rendezvous_node)
                        {
                            for ch in list {
                                let _ = ch.send(Err(std::io::Error::other(
                                    crate::error::rendezvous::Error::from(error),
                                )));
                            }
                        }
                    }
                }
            }
            RendezvousClientEvent::Registered {
                rendezvous_node,
                ttl,
                namespace,
            } => {
                tracing::debug!(%rendezvous_node, %namespace, %ttl, "registered to namespace");
                if let Some(list) = self
                    .pending_rendezvous_register
                    .shift_remove(&(rendezvous_node, namespace))
                {
                    for ch in list {
                        let _ = ch.send(Ok(()));
                    }
                }
            }
            RendezvousClientEvent::RegisterFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                tracing::error!(%rendezvous_node, %namespace, ?error, "failed to register to namespace");
                if let Some(list) = self
                    .pending_rendezvous_register
                    .shift_remove(&(rendezvous_node, namespace))
                {
                    for ch in list {
                        let _ = ch.send(Err(std::io::Error::other(
                            crate::error::rendezvous::Error::from(error),
                        )));
                    }
                }
            }
            RendezvousClientEvent::Expired { peer } => {
                tracing::debug!(%peer, "expired");
            }
        }
    }
}
