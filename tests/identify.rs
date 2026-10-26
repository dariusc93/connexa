#![cfg(feature = "identify")]

mod common;

use connexa::behaviour::BehaviourEvent;
use connexa::prelude::identify::Event as IdentifyEvent;
use connexa::prelude::swarm::SwarmEvent;
use connexa::prelude::{ConnexaSwarmEvent, StreamProtocol};
use futures::StreamExt;
use futures::channel::mpsc;

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn exchange_identity_and_metadata() {
    common::timeout(async {
        let (first_tx, mut first_events) = mpsc::unbounded();
        let (first, first_id, first_addr) = common::spawn_connexa_with_config(|builder| {
            builder
                .with_identify_with_config("/connexa/first/1", |config| {
                    config.with_agent_version("first agent".into())
                })
                .set_swarm_event_callback(move |_, _, event, _| {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Identify(
                        IdentifyEvent::Received { peer_id, info, .. },
                    )) = event
                    {
                        let _ = first_tx.unbounded_send((*peer_id, info.clone()));
                    }
                })
        })
        .await;
        let (second_tx, mut second_events) = mpsc::unbounded();
        let (second, second_id, second_addr) = common::spawn_connexa_with_config(|builder| {
            builder
                .with_identify_with_config("/connexa/second/1", |config| {
                    config.with_agent_version("second agent".into())
                })
                .set_swarm_event_callback(move |_, _, event, _| {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Identify(
                        IdentifyEvent::Received { peer_id, info, .. },
                    )) = event
                    {
                        let _ = second_tx.unbounded_send((*peer_id, info.clone()));
                    }
                })
        })
        .await;

        first.swarm().dial(second_addr.clone()).await.unwrap();
        let (peer_id, info) = first_events
            .next()
            .await
            .expect("identify info is available");
        assert_eq!(peer_id, second_id);
        assert_eq!(info.public_key, second.keypair().public());
        assert_eq!(info.protocol_version, "/connexa/second/1");
        assert_eq!(info.agent_version, "second agent");
        assert!(info.listen_addrs.contains(&second_addr));
        assert!(
            info.protocols
                .contains(&StreamProtocol::new("/ipfs/id/1.0.0"))
        );

        let (peer_id, info) = second_events
            .next()
            .await
            .expect("identify info is available");
        assert_eq!(peer_id, first_id);
        assert_eq!(info.public_key, first.keypair().public());
        assert_eq!(info.protocol_version, "/connexa/first/1");
        assert_eq!(info.agent_version, "first agent");
        assert!(info.listen_addrs.contains(&first_addr));
    })
    .await;
}

#[cfg(feature = "stream")]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn advertise_registered_stream_protocols() {
    common::timeout(async {
        let (tx, mut events) = mpsc::unbounded();
        let (node, _, _) = common::spawn_connexa_with_config(|builder| {
            builder
                .with_identify()
                .set_swarm_event_callback(move |_, _, event, _| {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Identify(
                        IdentifyEvent::Received { peer_id, info, .. },
                    )) = event
                    {
                        let _ = tx.unbounded_send((*peer_id, info.clone()));
                    }
                })
        })
        .await;
        let (peer, peer_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_identify().with_streams())
                .await;
        let protocol = StreamProtocol::new("/connexa/advertised/1");
        let _incoming = peer.stream().new_stream(protocol.clone()).await.unwrap();

        node.swarm().dial(addr).await.unwrap();
        let (id, info) = events.next().await.expect("identify info is available");
        assert_eq!(id, peer_id);
        assert!(info.protocols.contains(&protocol));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn store_addresses_discovered_by_identify() {
    common::timeout(async {
        let (tx, mut discovered) = mpsc::unbounded();
        let (node, _, _) = common::spawn_connexa_with_config(|builder| {
            builder
                .with_identify()
                .with_peer_store()
                .set_swarm_event_callback(move |_, _, event, _| {
                    if let SwarmEvent::NewExternalAddrOfPeer { peer_id, address } = event {
                        let _ = tx.unbounded_send((*peer_id, address.clone()));
                    }
                })
        })
        .await;
        let (peer, peer_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_identify()).await;
        let listener_id = peer.swarm().listen_on("/memory/0").await.unwrap();
        let advertised = peer.swarm().get_listening_addresses(listener_id).await.unwrap();
        let advertised = advertised.into_iter().next().unwrap();
        peer.swarm().add_external_address(advertised.clone()).await.unwrap();

        assert!(node.peer_store().list_all().await.unwrap().is_empty());
        node.swarm().dial(addr).await.unwrap();
        common::next_matching(&mut discovered, |(id, address)| {
            *id == peer_id && address == &advertised
        })
        .await;
        assert!(node.peer_store().list(peer_id).await.unwrap().contains(&advertised));

        let mut events = node.swarm().listener().await.unwrap();
        node.swarm().disconnect(peer_id).await.unwrap();
        common::next_matching(&mut events, |event| {
            matches!(event, ConnexaSwarmEvent::ConnectionClosed { peer_id: id, num_established: 0, .. } if *id == peer_id)
        })
        .await;
        node.swarm().dial(peer_id).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}
