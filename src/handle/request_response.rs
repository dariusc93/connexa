use crate::handle::Connexa;
use crate::types::RequestResponseCommand;
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use indexmap::IndexSet;
use libp2p::request_response::InboundRequestId;
use libp2p::{PeerId, StreamProtocol};
use std::io::Result as IoResult;

#[derive(Copy, Clone)]
pub struct ConnexaRequestResponse<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaRequestResponse<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    /// Listen for incoming requests on a specified protocol
    pub async fn listen_for_requests(
        &self,
        protocol: impl Into<OptionalStreamProtocol>,
    ) -> std::io::Result<BoxStream<'static, (PeerId, InboundRequestId, Bytes)>> {
        let protocol = protocol.into().into_inner();
        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(RequestResponseCommand::ListenForRequests { protocol, resp: tx }.into())
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(StreamExt::boxed)
    }

    /// Send a request to multiple peers, returning a stream of responses
    pub async fn send_requests(
        &self,
        peers: impl IntoIterator<Item = PeerId>,
        request: impl IntoRequest,
    ) -> std::io::Result<BoxStream<'static, (PeerId, std::io::Result<Bytes>)>> {
        let peers = IndexSet::from_iter(peers);
        let (protocol, request) = request.into_request()?;
        let protocol = protocol.into_inner();

        if peers.is_empty() {
            return Err(std::io::Error::other("no peers were provided"));
        }
        if request.is_empty() {
            return Err(std::io::Error::other("request is empty"));
        }

        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                RequestResponseCommand::SendRequests {
                    protocol,
                    peers,
                    request,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await
            .map_err(std::io::Error::other)?
            .map(StreamExt::boxed)
    }

    /// Send a request to a single peer and await the response
    pub async fn send_request(
        &self,
        peer_id: PeerId,
        request: impl IntoRequest,
    ) -> std::io::Result<Bytes> {
        let (protocol, request) = request.into_request()?;
        let protocol = protocol.into_inner();

        if request.is_empty() {
            return Err(std::io::Error::other("request is empty"));
        }

        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                RequestResponseCommand::SendRequest {
                    protocol,
                    peer_id,
                    request,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        let fut = rx.await.map_err(std::io::Error::other)??;
        fut.await
    }

    /// Send a response to a specific inbound request
    pub async fn send_response(
        &self,
        peer_id: PeerId,
        request_id: InboundRequestId,
        response: impl IntoRequest,
    ) -> std::io::Result<()> {
        let (protocol, response) = response.into_request()?;
        let protocol = protocol.into_inner();

        if response.is_empty() {
            return Err(std::io::Error::other("response is empty"));
        }

        let (tx, rx) = oneshot::channel();

        self.connexa
            .to_task
            .clone()
            .send(
                RequestResponseCommand::SendResponse {
                    protocol,
                    peer_id,
                    request_id,
                    response,
                    resp: tx,
                }
                .into(),
            )
            .await?;

        rx.await.map_err(std::io::Error::other)?
    }
}

pub struct OptionalStreamProtocol(pub(crate) Option<StreamProtocol>);

impl OptionalStreamProtocol {
    pub(crate) fn into_inner(self) -> Option<StreamProtocol> {
        self.0
    }
}

impl From<()> for OptionalStreamProtocol {
    fn from(_: ()) -> Self {
        Self(None)
    }
}

impl From<StreamProtocol> for OptionalStreamProtocol {
    fn from(protocol: StreamProtocol) -> Self {
        Self(Some(protocol))
    }
}

impl From<Option<StreamProtocol>> for OptionalStreamProtocol {
    fn from(protocol: Option<StreamProtocol>) -> Self {
        Self(protocol)
    }
}

impl From<String> for OptionalStreamProtocol {
    fn from(protocol: String) -> Self {
        let protocol = StreamProtocol::try_from_owned(protocol).ok();
        Self(protocol)
    }
}

impl From<&String> for OptionalStreamProtocol {
    fn from(protocol: &String) -> Self {
        let protocol = StreamProtocol::try_from_owned(protocol.to_string()).ok();
        Self(protocol)
    }
}

impl From<&'static str> for OptionalStreamProtocol {
    fn from(protocol: &'static str) -> Self {
        let protocol = StreamProtocol::new(protocol);
        Self(Some(protocol))
    }
}

// TODO: Move into a macro
pub trait IntoRequest {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)>;
}

impl IntoRequest for Bytes {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        Ok((None.into(), self))
    }
}

impl<const N: usize> IntoRequest for [u8; N] {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(Bytes::copy_from_slice(&self))
    }
}

impl<const N: usize> IntoRequest for &[u8; N] {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(Bytes::copy_from_slice(self))
    }
}

impl IntoRequest for Vec<u8> {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(Bytes::from(self))
    }
}

impl IntoRequest for &[u8] {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(Bytes::copy_from_slice(self))
    }
}

impl IntoRequest for String {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(Bytes::from(self.into_bytes()))
    }
}

impl IntoRequest for &'static str {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request(self.to_string())
    }
}

impl<F> IntoRequest for F
where
    F: FnOnce() -> IoResult<Bytes>,
{
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((None, self))
    }
}

impl<P: Into<OptionalStreamProtocol>> IntoRequest for (P, Bytes) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        let (protocol, request) = self;
        Ok((protocol.into(), request))
    }
}

impl<const N: usize, P: Into<OptionalStreamProtocol>> IntoRequest for (P, [u8; N]) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0.into(), Bytes::copy_from_slice(&self.1)))
    }
}

impl<const N: usize, P: Into<OptionalStreamProtocol>> IntoRequest for (P, &[u8; N]) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0.into(), Bytes::copy_from_slice(self.1)))
    }
}

impl<P: Into<OptionalStreamProtocol>> IntoRequest for (P, Vec<u8>) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0, Bytes::from(self.1)))
    }
}

impl<P: Into<OptionalStreamProtocol>> IntoRequest for (P, &[u8]) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0.into(), Bytes::copy_from_slice(self.1)))
    }
}

impl<P: Into<OptionalStreamProtocol>> IntoRequest for (P, String) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0.into(), self.1.into_bytes()))
    }
}

impl<P: Into<OptionalStreamProtocol>> IntoRequest for (P, &'static str) {
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        IntoRequest::into_request((self.0.into(), self.1.to_string()))
    }
}

impl<P: Into<OptionalStreamProtocol>, F> IntoRequest for (P, F)
where
    F: FnOnce() -> IoResult<Bytes>,
{
    fn into_request(self) -> IoResult<(OptionalStreamProtocol, Bytes)> {
        let (protocol, request) = self;
        request().map(|request| (protocol.into(), request))
    }
}
