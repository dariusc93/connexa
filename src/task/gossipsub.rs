use crate::prelude::{GossipsubMessage, NetworkBehaviour, PubsubEvent};
use crate::task::ConnexaTask;
use libp2p::gossipsub::Event as GossipsubEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_gossipsub_event(&mut self, event: GossipsubEvent) {
        let (topic, event) = match event {
            GossipsubEvent::Message {
                propagation_source,
                message,
                message_id,
            } => {
                let topic = message.topic;
                let message = GossipsubMessage {
                    message_id,
                    propagated_source: propagation_source,
                    source: message.source,
                    sequence_number: message.sequence_number,
                    data: message.data.into(),
                    propagate_message: None,
                };

                let event = PubsubEvent::Message { message };

                (topic, event)
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                let event = PubsubEvent::Subscribed { peer_id };
                (topic, event)
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                let event = PubsubEvent::Unsubscribed { peer_id };
                (topic, event)
            }
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                tracing::info!(%peer_id, "peer does not support gossipsub");
                return;
            }
            GossipsubEvent::SlowPeer {
                peer_id,
                failed_messages,
            } => {
                tracing::info!(%peer_id, ?failed_messages, "peer is slow");
                return;
            }
        };

        let Some(chs) = self.gossipsub_listener.get_mut(&topic) else {
            return;
        };

        // Should we retain the list too for any closed channels?
        for ch in chs {
            // TODO: Perform a check to see if the message needs propagation?
            let _ = ch.try_send(event.clone());
        }
    }
}
