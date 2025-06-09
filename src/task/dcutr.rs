use crate::prelude::NetworkBehaviour;
use crate::task::ConnexaTask;
use libp2p::dcutr::Event as DcutrEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_dcutr_event(&mut self, event: DcutrEvent) {
        let DcutrEvent {
            remote_peer_id,
            result,
        } = event;
        match result {
            Ok(remote_addr) => {
                tracing::info!(%remote_peer_id, %remote_addr, "dcutr success");
            }
            Err(e) => {
                tracing::error!(%remote_peer_id, %e, "dcutr failed");
            }
        }
    }
}
