use crate::handle::Connexa;
use crate::types::{FloodsubCommand, FloodsubMessage, PubsubEvent, PubsubFloodsubPublish};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::floodsub::Topic;

#[derive(Copy, Clone)]
pub struct ConnexaFloodsub<'a, T = ()> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaFloodsub<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Subscribes to a topic in the floodsub network
    pub async fn subscribe(&self, topic: impl IntoTopic) -> std::io::Result<()> {
        // TODO: avoid additional allocation
        let topic = topic.into_topic().id().to_string();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(FloodsubCommand::Subscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Creates a listener for a specific topic that returns a stream of pubsub events
    pub async fn listener(
        &self,
        topic: impl IntoTopic,
    ) -> std::io::Result<BoxStream<'static, PubsubEvent<FloodsubMessage>>> {
        let topic = topic.into_topic();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(FloodsubCommand::FloodsubListener { topic, resp: tx }.into())
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(|rx| rx.boxed())
    }

    /// Unsubscribes from a topic in the floodsub network
    pub async fn unsubscribe(&self, topic: impl IntoTopic) -> std::io::Result<()> {
        // TODO: avoid additional allocation
        let topic = topic.into_topic().id().to_string();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(FloodsubCommand::Unsubscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes a message to a single topic in the floodsub network
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
                FloodsubCommand::Publish(PubsubFloodsubPublish::Publish { topic, data }, tx).into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes a message to any peers in a single topic, regardless if they're subscribed
    pub async fn publish_any(
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
                FloodsubCommand::Publish(PubsubFloodsubPublish::PublishAny { topic, data }, tx)
                    .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes the same message to multiple topics in the floodsub network
    pub async fn publish_many(
        &self,
        topics: impl IntoIterator<Item = impl IntoTopic>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topics = topics
            .into_iter()
            .map(|t| t.into_topic())
            .collect::<Vec<_>>();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                FloodsubCommand::Publish(PubsubFloodsubPublish::PublishMany { topics, data }, tx)
                    .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes the same message to any peers in multiple topics, regardless if they're subscribed
    pub async fn publish_many_any(
        &self,
        topics: impl IntoIterator<Item = impl IntoTopic>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topics = topics
            .into_iter()
            .map(|t| t.into_topic())
            .collect::<Vec<_>>();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                FloodsubCommand::Publish(
                    PubsubFloodsubPublish::PublishManyAny { topics, data },
                    tx,
                )
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}

pub trait IntoTopic {
    fn into_topic(self) -> Topic;
}

impl IntoTopic for String {
    fn into_topic(self) -> Topic {
        Topic::new(self)
    }
}

impl IntoTopic for &String {
    fn into_topic(self) -> Topic {
        Topic::new(self)
    }
}

impl IntoTopic for &str {
    fn into_topic(self) -> Topic {
        Topic::new(self)
    }
}

impl IntoTopic for Topic {
    fn into_topic(self) -> Topic {
        self
    }
}

impl IntoTopic for &Topic {
    fn into_topic(self) -> Topic {
        self.clone()
    }
}

impl IntoTopic for Vec<u8> {
    fn into_topic(self) -> Topic {
        let topic = String::from_utf8_lossy(&self);
        Topic::new(topic)
    }
}

impl IntoTopic for &[u8] {
    fn into_topic(self) -> Topic {
        let topic = String::from_utf8_lossy(self);
        Topic::new(topic)
    }
}

impl IntoTopic for Bytes {
    fn into_topic(self) -> Topic {
        let topic = String::from_utf8_lossy(&self);
        Topic::new(topic)
    }
}

impl IntoTopic for &Bytes {
    fn into_topic(self) -> Topic {
        let topic = String::from_utf8_lossy(self);
        Topic::new(topic)
    }
}

// TODO: impl IntoTopic for Vec<String> and Vec<&str> and join with a dash?
