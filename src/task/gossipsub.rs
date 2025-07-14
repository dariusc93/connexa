use crate::prelude::{GossipsubMessage, PubsubEvent};
use crate::task::ConnexaTask;
use crate::types::GossipsubCommand;
use futures::channel::mpsc;
use libp2p::gossipsub::Event as GossipsubEvent;
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_gossipsub_command(&mut self, command: GossipsubCommand) {
        let swarm = self.swarm.as_mut().unwrap();

        match command {
            GossipsubCommand::Subscribe { topic, resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                };

                let topic = libp2p::gossipsub::IdentTopic::new(topic.into_string());
                match pubsub.subscribe(&topic) {
                    Ok(true) => {
                        let _ = resp.send(Ok(()));
                    }
                    Ok(false) => {
                        let _ = resp.send(Err(std::io::Error::other("topic already subscribed")));
                    }
                    Err(e) => {
                        let _ = resp.send(Err(std::io::Error::other(e)));
                    }
                }
            }
            GossipsubCommand::Unsubscribe { topic, resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                };

                let topic = libp2p::gossipsub::IdentTopic::new(topic.into_string());
                match pubsub.unsubscribe(&topic) {
                    true => {
                        let _ = resp.send(Ok(()));
                    }
                    false => {
                        let _ = resp.send(Err(std::io::Error::other("not subscribed to topic")));
                    }
                }
            }
            GossipsubCommand::Peers { topic, resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                };

                let peers = pubsub.mesh_peers(&topic).copied().collect();

                let _ = resp.send(Ok(peers));
            }
            GossipsubCommand::Subscribed { resp, .. } => {
                let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                };

                let topics = pubsub.topics().cloned().collect();

                let _ = resp.send(Ok(topics));
            }
            GossipsubCommand::Publish { topic, data, resp } => {
                let Some(pubsub) = swarm.behaviour_mut().gossipsub.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                };

                let ret = match pubsub.publish(topic, data) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(std::io::Error::other(e)),
                };

                let _ = resp.send(ret);
            }
            GossipsubCommand::GossipsubListener { topic, resp } => {
                if !swarm.behaviour_mut().gossipsub.is_enabled() {
                    let _ = resp.send(Err(std::io::Error::other("gossipsub is not enabled")));
                    return;
                }

                let (tx, rx) = mpsc::channel(50);

                self.gossipsub_listener.entry(topic).or_default().push(tx);

                let _ = resp.send(Ok(rx));
            }
        }
    }

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
