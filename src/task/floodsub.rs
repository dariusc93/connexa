use crate::prelude::{FloodsubMessage, NetworkBehaviour, PubsubEvent};
use crate::task::ConnexaTask;
use libp2p::floodsub::FloodsubEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_floodsub_event(&mut self, event: FloodsubEvent) {
        let (topics, event) = match event {
            FloodsubEvent::Message(libp2p::floodsub::FloodsubMessage {
                source,
                data,
                sequence_number,
                topics,
            }) => {
                let message = FloodsubMessage {
                    source,
                    data,
                    sequence_number,
                };

                let event = PubsubEvent::Message { message };

                (topics, event)
            }
            FloodsubEvent::Subscribed { peer_id, topic } => {
                let event = PubsubEvent::Subscribed { peer_id };
                (vec![topic], event)
            }
            FloodsubEvent::Unsubscribed { peer_id, topic } => {
                let event = PubsubEvent::Unsubscribed { peer_id };
                (vec![topic], event)
            }
        };

        for topic in topics {
            let Some(chs) = self.floodsub_listener.get_mut(&topic) else {
                continue;
            };

            // Should we retain the list too for any closed channels?

            for ch in chs {
                let _ = ch.try_send(event.clone());
            }
        }
    }
}
