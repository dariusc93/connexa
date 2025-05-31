use crate::handle::Connexa;
use crate::types::RequestResponseCommand;
use bytes::Bytes;
use futures::StreamExt;
use futures::channel::oneshot;
use futures::stream::BoxStream;
use indexmap::IndexSet;
use libp2p::request_response::InboundRequestId;
use libp2p::{PeerId, StreamProtocol};

pub struct ConnexaRequestResponse<'a> {
    connexa: &'a Connexa,
}

impl<'a> ConnexaRequestResponse<'a> {
    pub(crate) fn new(connexa: &'a Connexa) -> Self {
        Self { connexa }
    }

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

    pub async fn send_requests(
        &self,
        peers: impl IntoIterator<Item = PeerId>,
        request: impl IntoRequest,
    ) -> std::io::Result<BoxStream<'static, (PeerId, std::io::Result<Bytes>)>> {
        let peers = IndexSet::from_iter(peers);
        let (protocol, request) = request.into_request();

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

    pub async fn send_request(
        &self,
        peer_id: PeerId,
        request: impl IntoRequest,
    ) -> std::io::Result<Bytes> {
        let (protocol, request) = request.into_request();

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

    pub async fn send_response(
        &self,
        peer_id: PeerId,
        request_id: InboundRequestId,
        response: impl IntoRequest,
    ) -> std::io::Result<()> {
        let (protocol, response) = response.into_request();
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

impl From<String> for OptionalStreamProtocol {
    fn from(protocol: String) -> Self {
        let protocol = StreamProtocol::try_from_owned(protocol).ok();
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
    fn into_request(self) -> (Option<StreamProtocol>, Bytes);
}

impl IntoRequest for Bytes {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        (None, self)
    }
}

impl<const N: usize> IntoRequest for [u8; N] {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(Bytes::copy_from_slice(&self))
    }
}

impl<const N: usize> IntoRequest for &[u8; N] {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(Bytes::copy_from_slice(self))
    }
}

impl IntoRequest for Vec<u8> {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(Bytes::from(self))
    }
}

impl IntoRequest for &[u8] {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(Bytes::copy_from_slice(self))
    }
}

impl IntoRequest for String {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(Bytes::from(self.into_bytes()))
    }
}

impl IntoRequest for &'static str {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request(self.to_string())
    }
}

impl IntoRequest for (StreamProtocol, Bytes) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        (Some(self.0), self.1)
    }
}

impl<const N: usize> IntoRequest for (StreamProtocol, [u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(&self.1)))
    }
}

impl<const N: usize> IntoRequest for (StreamProtocol, &[u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (StreamProtocol, Vec<u8>) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1)))
    }
}

impl IntoRequest for (StreamProtocol, &[u8]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (StreamProtocol, String) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1.into_bytes())))
    }
}

impl IntoRequest for (StreamProtocol, &'static str) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, self.1.to_string()))
    }
}

impl IntoRequest for (String, Bytes) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        (
            Some(StreamProtocol::try_from_owned(self.0).expect("valid protocol")),
            self.1,
        )
    }
}

impl<const N: usize> IntoRequest for (String, [u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(&self.1)))
    }
}

impl<const N: usize> IntoRequest for (String, &[u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (String, Vec<u8>) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1)))
    }
}

impl IntoRequest for (String, &[u8]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (String, String) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1.into_bytes())))
    }
}

impl IntoRequest for (String, &'static str) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, self.1.to_string()))
    }
}

impl IntoRequest for (&'static str, Bytes) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        (Some(StreamProtocol::new(self.0)), self.1)
    }
}

impl<const N: usize> IntoRequest for (&'static str, [u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(&self.1)))
    }
}

impl<const N: usize> IntoRequest for (&'static str, &[u8; N]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (&'static str, Vec<u8>) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1)))
    }
}

impl IntoRequest for (&'static str, &[u8]) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::copy_from_slice(self.1)))
    }
}

impl IntoRequest for (&'static str, String) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, Bytes::from(self.1.into_bytes())))
    }
}

impl IntoRequest for (&'static str, &'static str) {
    fn into_request(self) -> (Option<StreamProtocol>, Bytes) {
        IntoRequest::into_request((self.0, self.1.to_string()))
    }
}
