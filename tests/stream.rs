#![cfg(feature = "stream")]

mod common;

use connexa::error::{Error, Protocol};
use connexa::prelude::stream::OpenStreamError;
use connexa::prelude::{PeerId, StreamProtocol};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn exchange_bytes_over_a_stream() {
    common::timeout(async {
        let (sender, sender_id, _) =
            common::spawn_connexa_with_config(|builder| builder.with_streams()).await;
        let (receiver, receiver_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_streams()).await;
        let protocol = StreamProtocol::new("/connexa/stream/1");
        let mut incoming = receiver
            .stream()
            .new_stream(protocol.clone())
            .await
            .unwrap();
        sender.swarm().dial(addr).await.unwrap();
        let payload = vec![42; 128 * 1024];

        let send = async {
            let mut stream = sender
                .stream()
                .open_stream(receiver_id, protocol)
                .await
                .unwrap();
            stream.write_all(&payload).await.unwrap();
            stream.close().await.unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            assert_eq!(response, b"received");
        };
        let receive = async {
            let (peer_id, mut stream) =
                incoming.next().await.expect("incoming stream is available");
            assert_eq!(peer_id, sender_id);
            let mut received = Vec::new();
            stream.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, payload);
            stream.write_all(b"received").await.unwrap();
            stream.close().await.unwrap();
        };
        futures::join!(send, receive);
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn unsupported_protocol_returns_an_error() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_streams()).await;
        let (_peer, peer_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_streams()).await;
        node.swarm().dial(addr).await.unwrap();
        let protocol = StreamProtocol::new("/connexa/missing/1");
        let Error::Io(error) = node
            .stream()
            .open_stream(peer_id, protocol.clone())
            .await
            .unwrap_err()
        else {
            panic!("opening the stream returns an IO error");
        };
        assert!(matches!(
            error.get_ref().and_then(|error| error.downcast_ref::<OpenStreamError>()),
            Some(OpenStreamError::UnsupportedProtocol(actual)) if actual == &protocol
        ));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn dropping_a_listener_allows_registering_the_protocol_again() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_streams()).await;
        let protocol = StreamProtocol::new("/connexa/stream/1");
        let incoming = node.stream().new_stream(protocol.clone()).await.unwrap();
        let Err(Error::Io(error)) = node.stream().new_stream(protocol.clone()).await else {
            panic!("duplicate protocol registration returns an IO error");
        };
        assert!(
            error
                .get_ref()
                .unwrap()
                .is::<libp2p_stream::AlreadyRegistered>()
        );

        drop(incoming);
        let _incoming = node.stream().new_stream(protocol).await.unwrap();
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_streams_return_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let protocol = StreamProtocol::new("/connexa/stream/1");
        for result in [
            node.stream().new_stream(protocol.clone()).await.map(|_| ()),
            node.stream().control_handle().await.map(|_| ()),
            node.stream()
                .open_stream(PeerId::random(), protocol)
                .await
                .map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Stream
                })
            ));
        }
    })
    .await;
}
