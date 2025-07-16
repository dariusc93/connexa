use crate::handle::Connexa;
use crate::types::StreamCommand;
use bytes::Bytes;
use futures::channel::oneshot;
use libp2p::{PeerId, StreamProtocol};

#[derive(Copy, Clone)]
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

    /// Creates a new stream with the specified protocol
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

    /// Gets a control handle for managing streams
    pub async fn control_handle(&self) -> std::io::Result<libp2p_stream::Control> {
        let (tx, rx) = oneshot::channel();
        self.connexa
            .to_task
            .clone()
            .send(StreamCommand::ControlHandle { resp: tx }.into())
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }

    /// Opens a stream with the specified protocol
    pub async fn open_stream(
        &self,
        peer_id: PeerId,
        protocol: impl IntoStreamProtocol,
    ) -> std::io::Result<libp2p::Stream> {
        let protocol: StreamProtocol = protocol.into_protocol()?;

        let mut control = self.control_handle().await?;
        let stream = control
            .open_stream(peer_id, protocol)
            .await
            .map_err(std::io::Error::other)?;
        Ok(stream)
    }
}

pub trait IntoStreamProtocol {
    fn into_protocol(self) -> std::io::Result<StreamProtocol>;
}

impl IntoStreamProtocol for StreamProtocol {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        Ok(self)
    }
}

impl IntoStreamProtocol for String {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        StreamProtocol::try_from_owned(self).map_err(std::io::Error::other)
    }
}

impl IntoStreamProtocol for &String {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        StreamProtocol::try_from_owned(self.to_owned()).map_err(std::io::Error::other)
    }
}

impl IntoStreamProtocol for &'static str {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        Ok(StreamProtocol::new(self))
    }
}

impl IntoStreamProtocol for Bytes {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        let protocol = String::from_utf8_lossy(&self).to_string();
        protocol.into_protocol()
    }
}

impl IntoStreamProtocol for &Bytes {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        let protocol = String::from_utf8_lossy(self).to_string();
        protocol.into_protocol()
    }
}

impl IntoStreamProtocol for Vec<u8> {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        let protocol = String::from_utf8_lossy(&self).to_string();
        protocol.into_protocol()
    }
}

impl IntoStreamProtocol for &[u8] {
    fn into_protocol(self) -> std::io::Result<StreamProtocol> {
        let protocol = String::from_utf8_lossy(&self).to_string();
        protocol.into_protocol()
    }
}
