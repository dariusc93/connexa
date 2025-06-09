use crate::prelude::NetworkBehaviour;
use crate::task::ConnexaTask;
use libp2p::relay::{Event as RelayServerEvent, client::Event as RelayClientEvent};
use std::fmt::Debug;
impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
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
