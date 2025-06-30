use crate::task::ConnexaTask;
use libp2p::mdns::Event as MdnsEvent;
use std::fmt::Debug;

use libp2p::swarm::NetworkBehaviour;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_mdns_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered) => {
                for (peer_id, addr) in discovered {
                    tracing::info!(%peer_id, %addr, "peer discovered");
                }
            }
            MdnsEvent::Expired(expired) => {
                for (peer_id, addr) in expired {
                    tracing::info!(%peer_id, %addr, "peer expired");
                }
            }
        }
    }
}
