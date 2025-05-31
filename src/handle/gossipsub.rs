use crate::handle::Connexa;
use crate::types::{GossipsubMessage, PubsubCommand, PubsubEvent, PubsubPublishType};
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use libp2p::PeerId;

pub struct ConnexaGossipsub<'a> {
    connexa: &'a Connexa,
}

impl<'a> ConnexaGossipsub<'a> {
    pub(crate) fn new(connexa: &'a Connexa) -> Self {
        Self { connexa }
    }

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
