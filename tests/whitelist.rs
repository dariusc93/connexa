mod common;

use connexa::error::{Error, Protocol};
use connexa::prelude::swarm::{DialError, ListenError};
use connexa::prelude::{ConnexaSwarmEvent, DefaultConnexaBuilder, PeerId};

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn manage_whitelisted_peers() {
    common::timeout(async {
        let first = PeerId::random();
        let second = PeerId::random();
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_whitelist_with_list([first]))
                .await;

        assert_eq!(node.whitelist().list().await.unwrap(), vec![first]);
        node.whitelist().add(second).await.unwrap();
        let peers = node.whitelist().list().await.unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&first));
        assert!(peers.contains(&second));
        assert!(matches!(
            node.whitelist().add(second).await,
            Err(Error::AlreadyExists(_))
        ));

        node.whitelist().remove(first).await.unwrap();
        assert_eq!(node.whitelist().list().await.unwrap(), vec![second]);
        assert!(matches!(
            node.whitelist().remove(first).await,
            Err(Error::NotFound(_))
        ));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn whitelist_controls_outbound_connections() {
    common::timeout(async {
        let (_peer, peer_id, addr) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_whitelist()).await;

        let Error::Dial(error) = node.swarm().dial(addr.clone()).await.unwrap_err() else {
            panic!("dial returns a connection error");
        };
        let DialError::Denied { cause } = error.as_ref() else {
            panic!("whitelist denies the connection");
        };
        assert!(cause.downcast_ref::<libp2p_allow_block_list::NotAllowed>().is_some());

        node.whitelist().add(peer_id).await.unwrap();
        node.swarm().dial(addr.clone()).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());

        let mut events = node.swarm().listener().await.unwrap();
        node.whitelist().remove(peer_id).await.unwrap();
        common::next_matching(&mut events, |event| {
            matches!(event, ConnexaSwarmEvent::ConnectionClosed { peer_id: id, num_established: 0, .. } if *id == peer_id)
        })
        .await;
        assert!(!node.swarm().is_connected(peer_id).await.unwrap());
        assert!(matches!(node.swarm().dial(addr).await, Err(Error::Dial(_))));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn whitelist_rejects_inbound_connections() {
    common::timeout(async {
        let (peer, peer_id, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_whitelist()).await;
        let mut events = node.swarm().listener().await.unwrap();

        let _ = peer.swarm().dial(addr).await;
        let event = common::next_matching(&mut events, |event| {
            matches!(event, ConnexaSwarmEvent::IncomingConnectionError { peer_id: Some(id), .. } if *id == peer_id)
        })
        .await;
        let ConnexaSwarmEvent::IncomingConnectionError { error, .. } = event else {
            unreachable!();
        };
        let ListenError::Denied { cause } = error.as_ref() else {
            panic!("whitelist denies the inbound connection");
        };
        assert!(cause.downcast_ref::<libp2p_allow_block_list::NotAllowed>().is_some());
        assert!(!node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_whitelist_returns_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let peer_id = PeerId::random();
        for result in [
            node.whitelist().add(peer_id).await,
            node.whitelist().remove(peer_id).await,
            node.whitelist().list().await.map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Other("whitelist")
                })
            ));
        }
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn whitelist_and_blacklist_cannot_be_enabled_together() {
    common::timeout(async {
        let result = DefaultConnexaBuilder::new_identity()
            .enable_memory_transport()
            .with_whitelist()
            .with_blacklist()
            .build()
            .await;
        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    })
    .await;
}
