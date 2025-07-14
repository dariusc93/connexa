use crate::prelude::{FloodsubMessage, PubsubEvent, PubsubFloodsubPublish};
use crate::task::ConnexaTask;
use crate::types::FloodsubCommand;
use futures::channel::mpsc;
use libp2p::floodsub::Event as FloodsubEvent;
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_floodsub_command(&mut self, command: FloodsubCommand) {
        let swarm = self.swarm.as_mut().unwrap();

        match command {
            FloodsubCommand::Subscribe { topic, resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                    return;
                };

                let topic = libp2p::floodsub::Topic::new(topic);
                match pubsub.subscribe(topic) {
                    true => {
                        let _ = resp.send(Ok(()));
                    }
                    false => {
                        let _ = resp.send(Err(std::io::Error::other("topic already subscribed")));
                    }
                }
            }
            FloodsubCommand::Unsubscribe { topic, resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                    return;
                };

                let topic = libp2p::floodsub::Topic::new(topic);
                match pubsub.unsubscribe(topic) {
                    true => {
                        let _ = resp.send(Ok(()));
                    }
                    false => {
                        let _ = resp.send(Err(std::io::Error::other("not subscribed to topic")));
                    }
                }
            }
            FloodsubCommand::Publish(pubsub_type, resp) => {
                let Some(pubsub) = swarm.behaviour_mut().floodsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                    return;
                };

                match pubsub_type {
                    PubsubFloodsubPublish::Publish { topic, data } => {
                        let topic = libp2p::floodsub::Topic::new(topic);
                        pubsub.publish(topic, data);
                    }
                    PubsubFloodsubPublish::PublishAny { topic, data } => {
                        let topic = libp2p::floodsub::Topic::new(topic);
                        pubsub.publish_any(topic, data);
                    }
                    PubsubFloodsubPublish::PublishMany { topics, data } => {
                        let topics = topics.into_iter().map(libp2p::floodsub::Topic::new);
                        pubsub.publish_many(topics, data);
                    }
                    PubsubFloodsubPublish::PublishManyAny { topics, data } => {
                        let topics = topics.into_iter().map(libp2p::floodsub::Topic::new);
                        pubsub.publish_many_any(topics, data);
                    }
                }

                let _ = resp.send(Ok(()));
            }
            FloodsubCommand::FloodsubListener { topic, resp } => {
                if !swarm.behaviour_mut().floodsub.is_enabled() {
                    let _ = resp.send(Err(std::io::Error::other("floodsub is not enabled")));
                    return;
                }

                let topic = libp2p::floodsub::Topic::new(topic);

                let (tx, rx) = mpsc::channel(50);

                self.floodsub_listener.entry(topic).or_default().push(tx);

                let _ = resp.send(Ok(rx));
            }
        }
    }

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
