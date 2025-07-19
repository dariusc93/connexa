use crate::handle::Connexa;
use crate::types::{GossipsubCommand, GossipsubEvent};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::PeerId;
use libp2p::gossipsub::{Hasher, IdentTopic, MessageAcceptance, MessageId, Topic, TopicHash};

#[derive(Copy, Clone)]
pub struct ConnexaGossipsub<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaGossipsub<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Subscribes to a specified topic in the gossipsub network.
    pub async fn subscribe(&self, topic: impl IntoTopic) -> std::io::Result<()> {
        let topic = topic.into_topic();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(GossipsubCommand::Subscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Creates a listener for a specified gossipsub topic.
    pub async fn listener(
        &self,
        topic: impl IntoTopic,
    ) -> std::io::Result<BoxStream<'static, GossipsubEvent>> {
        let topic = topic.into_topic();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(GossipsubCommand::GossipsubListener { topic, resp: tx }.into())
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(|rx| rx.boxed())
    }

    /// Unsubscribes from a specified gossipsub topic.
    pub async fn unsubscribe(&self, topic: impl IntoTopic) -> std::io::Result<()> {
        let topic = topic.into_topic();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(GossipsubCommand::Unsubscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Retrieves a list of peers that are subscribed to a specified topic.
    pub async fn peers(&self, topic: impl IntoTopic) -> std::io::Result<Vec<PeerId>> {
        let topic = topic.into_topic();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(GossipsubCommand::Peers { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes a message to a specified gossipsub topic.
    pub async fn publish(
        &self,
        topic: impl IntoTopic,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topic = topic.into_topic();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                GossipsubCommand::Publish {
                    topic,
                    data,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Reports validation results to the gossipsub system for a received message
    pub async fn report_message(
        &self,
        peer_id: PeerId,
        message_id: MessageId,
        message_acceptance: MessageAcceptance,
    ) -> std::io::Result<bool> {
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                GossipsubCommand::ReportMessage {
                    peer_id,
                    message_id,
                    accept: message_acceptance,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}

pub trait IntoTopic {
    fn into_topic(self) -> TopicHash;
}

impl<H: Hasher> IntoTopic for Topic<H> {
    fn into_topic(self) -> TopicHash {
        self.hash()
    }
}

impl<H: Hasher> IntoTopic for &Topic<H> {
    fn into_topic(self) -> TopicHash {
        self.hash()
    }
}

impl IntoTopic for TopicHash {
    fn into_topic(self) -> TopicHash {
        self
    }
}

impl IntoTopic for &TopicHash {
    fn into_topic(self) -> TopicHash {
        self.clone()
    }
}

impl IntoTopic for String {
    fn into_topic(self) -> TopicHash {
        IdentTopic::new(self).hash()
    }
}

impl IntoTopic for &String {
    fn into_topic(self) -> TopicHash {
        IdentTopic::new(self).hash()
    }
}

impl IntoTopic for &str {
    fn into_topic(self) -> TopicHash {
        IdentTopic::new(self).hash()
    }
}

impl IntoTopic for Vec<u8> {
    fn into_topic(self) -> TopicHash {
        let topic = String::from_utf8_lossy(&self);
        IdentTopic::new(topic).hash()
    }
}

impl IntoTopic for &[u8] {
    fn into_topic(self) -> TopicHash {
        let topic = String::from_utf8_lossy(self);
        IdentTopic::new(topic).hash()
    }
}

impl IntoTopic for Bytes {
    fn into_topic(self) -> TopicHash {
        let topic = String::from_utf8_lossy(&self);
        IdentTopic::new(topic).hash()
    }
}

impl IntoTopic for &Bytes {
    fn into_topic(self) -> TopicHash {
        let topic = String::from_utf8_lossy(self);
        IdentTopic::new(topic).hash()
    }
}

impl IntoTopic for Vec<String> {
    fn into_topic(self) -> TopicHash {
        let topic = self.join("/");
        IntoTopic::into_topic(topic)
    }
}

impl IntoTopic for &[String] {
    fn into_topic(self) -> TopicHash {
        let topic = self.join("/");
        IntoTopic::into_topic(topic)
    }
}

impl IntoTopic for &[&str] {
    fn into_topic(self) -> TopicHash {
        let topic = self.join("/");
        IntoTopic::into_topic(topic)
    }
}

impl IntoTopic for Vec<&str> {
    fn into_topic(self) -> TopicHash {
        let topic = self.join("/");
        IntoTopic::into_topic(topic)
    }
}
