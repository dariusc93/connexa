use crate::behaviour;
use crate::behaviour::BehaviourEvent;
use crate::prelude::ConnectionEvent;
use crate::task::ConnexaTask;
use libp2p::swarm::NetworkBehaviour;
use libp2p::swarm::SwarmEvent;
use std::collections::hash_map::Entry;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent<C>>) {
        let Some(swarm) = self.swarm.as_mut() else {
            return;
        };
        (self.swarm_event_callback)(swarm, &event, &mut self.context);
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
                self.connection_listeners.retain(|ch| !ch.is_closed());

                for ch in self.connection_listeners.iter_mut() {
                    if let Err(e) = ch.try_send(ConnectionEvent::ConnectionEstablished {
                        peer_id,
                        connection_id,
                        endpoint: endpoint.clone(),
                        established: num_established.get(),
                    }) {
                        tracing::warn!(%peer_id, %connection_id, ?endpoint, %num_established, error=%e, "failed to send connection established event");
                    }
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

                self.connection_listeners.retain(|ch| !ch.is_closed());

                for ch in self.connection_listeners.iter_mut() {
                    if let Err(e) = ch.try_send(ConnectionEvent::ConnectionClosed {
                        peer_id,
                        connection_id,
                        endpoint: endpoint.clone(),
                        num_established,
                    }) {
                        tracing::warn!(%peer_id, %connection_id, ?endpoint, %num_established, error=%e, "failed to send connection closed event");
                    }
                }
            }
            SwarmEvent::IncomingConnection {
                connection_id,
                local_addr,
                send_back_addr,
            } => {
                tracing::info!(%connection_id, ?local_addr, ?send_back_addr, "incoming connection");
            }
            SwarmEvent::IncomingConnectionError {
                peer_id,
                connection_id,
                local_addr,
                send_back_addr,
                error,
            } => {
                tracing::error!(?peer_id, %connection_id, ?local_addr, ?send_back_addr, error=%error, "incoming connection error");
            }
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

                self.listener_addresses
                    .entry(listener_id)
                    .or_default()
                    .push(address);
            }
            SwarmEvent::ExpiredListenAddr {
                listener_id,
                address,
            } => {
                // TODO: Determine if we should remove the address from external addresses
                tracing::info!(%listener_id, %address, "expired listen address");
                if let Entry::Occupied(mut entry) = self.listener_addresses.entry(listener_id) {
                    entry.get_mut().retain(|addr| *addr != address);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                reason,
            } => {
                tracing::info!(%listener_id, ?addresses, ?reason, "listener closed");
                self.listener_addresses.remove(&listener_id);
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
            SwarmEvent::Dialing {
                peer_id,
                connection_id,
            } => {
                tracing::trace!(?peer_id, %connection_id, "dialing");
            }
            SwarmEvent::NewExternalAddrCandidate { .. } => {}
            SwarmEvent::ExternalAddrConfirmed { address } => {
                tracing::debug!(%address, "external address confirmed");
            }
            SwarmEvent::ExternalAddrExpired { .. } => {}
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                tracing::debug!(%peer_id, %address, "new external address of peer");
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
            #[cfg(not(target_arch = "wasm32"))]
            #[cfg(feature = "upnp")]
            BehaviourEvent::Upnp(event) => self.process_upnp_event(event),
            #[cfg(not(target_arch = "wasm32"))]
            #[cfg(all(feature = "dcutr", feature = "relay"))]
            BehaviourEvent::Dcutr(event) => self.process_dcutr_event(event),
            #[cfg(feature = "rendezvous")]
            BehaviourEvent::RendezvousClient(event) => self.process_rendezvous_client_event(event),
            #[cfg(feature = "rendezvous")]
            BehaviourEvent::RendezvousServer(event) => self.process_rendezvous_server_event(event),
            #[cfg(not(target_arch = "wasm32"))]
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
}
