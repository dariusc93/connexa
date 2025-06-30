use crate::task::ConnexaTask;
use libp2p::relay::{Event as RelayServerEvent, client::Event as RelayClientEvent};
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_relay_client_event(&mut self, event: RelayClientEvent) {
        match event {
            RelayClientEvent::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                limit,
            } => {
                tracing::info!(%relay_peer_id, %renewal, ?limit, "relay client reservation request accepted");
            }
            RelayClientEvent::OutboundCircuitEstablished {
                relay_peer_id,
                limit,
            } => {
                tracing::info!(%relay_peer_id, ?limit, "relay client outbound circuit established");
            }
            RelayClientEvent::InboundCircuitEstablished { src_peer_id, limit } => {
                tracing::info!(%src_peer_id, ?limit, "relay client inbound circuit established");
            }
        }
    }

    pub fn process_relay_server_event(&mut self, event: RelayServerEvent) {
        match event {
            RelayServerEvent::ReservationReqAccepted {
                src_peer_id,
                renewed,
            } => {
                tracing::info!(%src_peer_id, %renewed, "relay server reservation request accepted");
            }
            RelayServerEvent::ReservationReqDenied {
                src_peer_id,
                status,
            } => {
                tracing::warn!(%src_peer_id, ?status, "relay server reservation request denied");
            }
            RelayServerEvent::ReservationTimedOut { src_peer_id } => {
                tracing::warn!(%src_peer_id, "relay server reservation timed out");
            }
            RelayServerEvent::CircuitReqDenied {
                src_peer_id,
                dst_peer_id,
                status,
            } => {
                tracing::warn!(%src_peer_id, %dst_peer_id, ?status, "relay server circuit request denied");
            }
            RelayServerEvent::CircuitReqAccepted {
                src_peer_id,
                dst_peer_id,
            } => {
                tracing::info!(%src_peer_id, %dst_peer_id, "relay server circuit request accepted");
            }
            RelayServerEvent::CircuitClosed {
                src_peer_id,
                dst_peer_id,
                error,
            } => {
                tracing::warn!(%src_peer_id, %dst_peer_id, ?error, "relay server circuit closed");
            }
            _ => {}
        }
    }
}
