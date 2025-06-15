use crate::prelude::NetworkBehaviour;
use crate::task::ConnexaTask;
use libp2p::rendezvous::Registration;
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
            }
            RendezvousClientEvent::DiscoverFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                tracing::error!(%rendezvous_node, ?namespace, ?error, "failed to discover ");
            }
            RendezvousClientEvent::Registered {
                rendezvous_node,
                ttl,
                namespace,
            } => {
                tracing::debug!(%rendezvous_node, %namespace, %ttl, "registered to namespace");
            }
            RendezvousClientEvent::RegisterFailed {
                rendezvous_node,
                namespace,
                error,
            } => {
                tracing::error!(%rendezvous_node, %namespace, ?error, "failed to register to namespace");
            }
            RendezvousClientEvent::Expired { peer } => {
                tracing::debug!(%peer, "expired");
            }
        }
    }
}
