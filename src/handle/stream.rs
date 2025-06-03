use crate::handle::Connexa;
use crate::types::StreamCommand;
use futures::channel::oneshot;
use libp2p::StreamProtocol;

pub struct ConnexaStream<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaStream<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    pub async fn new_stream(
        &self,
        protocol: StreamProtocol,
    ) -> std::io::Result<libp2p_stream::IncomingStreams> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(StreamCommand::NewStream { protocol, resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    pub async fn control_handle(&self) -> std::io::Result<libp2p_stream::Control> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(StreamCommand::ControlHandle { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}
