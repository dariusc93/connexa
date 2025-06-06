use crate::handle::Connexa;
use crate::types::{GossipsubMessage, PubsubCommand, PubsubEvent, PubsubPublishType};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::PeerId;

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

    /// Creates a listener for a specified gossipsub topic.
    pub async fn listener(
        &self,
        topic: impl Into<String>,
    ) -> std::io::Result<BoxStream<'static, PubsubEvent<GossipsubMessage>>> {
        let topic = topic.into();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(PubsubCommand::GossipsubListener { topic, resp: tx }.into())
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(|rx| rx.boxed())
    }

    /// Unsubscribes from a specified gossipsub topic.
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

    /// Retrieves a list of peers that are subscribed to a specified topic.
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

    /// Publishes a message to a specified gossipsub topic.
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
                PubsubCommand::Publish(PubsubPublishType::Gossipsub {
                    topic,
                    data,
                    resp: tx,
                })
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}
