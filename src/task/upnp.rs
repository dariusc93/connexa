use crate::behaviour::peer_store::store::Store;
use crate::task::ConnexaTask;
use libp2p::swarm::NetworkBehaviour;
use libp2p::upnp::Event as UpnpEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, S, T> ConnexaTask<X, C, S, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
    S: Store,
{
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
}
