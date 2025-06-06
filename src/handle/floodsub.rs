use crate::handle::Connexa;
use crate::types::{
    FloodsubMessage, PubsubCommand, PubsubEvent, PubsubFloodsubPublish, PubsubPublishType,
};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::PeerId;

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
    pub async fn subscribe(&self, topic: impl Into<String>) -> std::io::Result<()> {
        let topic = topic.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(PubsubCommand::Subscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Creates a listener for a specific topic that returns a stream of pubsub events
    pub async fn listener(
        &self,
        topic: impl Into<String>,
    ) -> std::io::Result<BoxStream<'static, PubsubEvent<FloodsubMessage>>> {
        let topic = topic.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(PubsubCommand::FloodsubListener { topic, resp: tx }.into())
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(|rx| rx.boxed())
    }

    /// Unsubscribes from a topic in the floodsub network
    pub async fn unsubscribe(&self, topic: impl Into<String>) -> std::io::Result<()> {
        let topic = topic.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(PubsubCommand::Unsubscribe { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Returns a list of peers subscribed to the specified topic
    pub async fn peers(&self, topic: impl Into<String>) -> std::io::Result<Vec<PeerId>> {
        let topic = topic.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(PubsubCommand::Peers { topic, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes a message to a single topic in the floodsub network
    pub async fn publish(
        &self,
        topic: impl Into<String>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topic = topic.into();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                PubsubCommand::Publish(PubsubPublishType::Floodsub(
                    PubsubFloodsubPublish::Publish { topic, data },
                    tx,
                ))
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes a message to any peers in a single topic, regardless if they're subscribed
    pub async fn publish_any(
        &self,
        topic: impl Into<String>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topic = topic.into();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                PubsubCommand::Publish(PubsubPublishType::Floodsub(
                    PubsubFloodsubPublish::PublishAny { topic, data },
                    tx,
                ))
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes the same message to multiple topics in the floodsub network
    pub async fn publish_many(
        &self,
        topics: impl IntoIterator<Item = impl Into<String>>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topics = topics.into_iter().map(|t| t.into()).collect::<Vec<_>>();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                PubsubCommand::Publish(PubsubPublishType::Floodsub(
                    PubsubFloodsubPublish::PublishMany { topics, data },
                    tx,
                ))
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Publishes the same message to any peers in multiple topics, regardless if they're subscribed
    pub async fn publish_many_any(
        &self,
        topics: impl IntoIterator<Item = impl Into<String>>,
        message: impl Into<Bytes>,
    ) -> std::io::Result<()> {
        let topics = topics.into_iter().map(|t| t.into()).collect::<Vec<_>>();
        let data = message.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                PubsubCommand::Publish(PubsubPublishType::Floodsub(
                    PubsubFloodsubPublish::PublishManyAny { topics, data },
                    tx,
                ))
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}
