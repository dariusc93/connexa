use crate::behaviour::peer_store::store::Store;
use crate::task::ConnexaTask;
use libp2p::ping::Event as PingEvent;
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, S, T> ConnexaTask<X, C, S, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
    S: Store,
{
    pub fn process_ping_event(&mut self, event: PingEvent) {
        let PingEvent {
            peer,
            connection,
            result,
        } = event;
        match result {
            Ok(duration) => {
                tracing::info!("ping to {} at {} took {:?}", peer, connection, duration);
            }
            Err(e) => {
                // TODO: Possibly disconnect peer since if there is an error?
                tracing::error!("ping to {} at {} failed: {:?}", peer, connection, e);
            }
        }
    }
}
