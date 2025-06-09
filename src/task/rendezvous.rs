use crate::prelude::NetworkBehaviour;
use crate::task::ConnexaTask;
use libp2p::rendezvous::client::Event as RendezvousClientEvent;
use libp2p::rendezvous::server::Event as RendezvousServerEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
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

    pub fn process_rendezvous_client_event(&mut self, event: RendezvousClientEvent) {
        match event {
            RendezvousClientEvent::Discovered { .. } => {}
            RendezvousClientEvent::DiscoverFailed { .. } => {}
            RendezvousClientEvent::Registered { .. } => {}
            RendezvousClientEvent::RegisterFailed { .. } => {}
            RendezvousClientEvent::Expired { .. } => {}
        }
    }
}
