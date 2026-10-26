mod common;

use connexa::error::{Error, Protocol};
use connexa::prelude::swarm::{DialError, ListenError};
use connexa::prelude::{ConnexaSwarmEvent, PeerId};

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn manage_blacklisted_peers() {
    common::timeout(async {
        let first = PeerId::random();
        let second = PeerId::random();
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_blacklist_with_list([first]))
                .await;

        assert_eq!(node.blacklist().list().await.unwrap(), vec![first]);
        node.blacklist().add(second).await.unwrap();
        let peers = node.blacklist().list().await.unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&first));
        assert!(peers.contains(&second));
        assert!(matches!(
            node.blacklist().add(second).await,
            Err(Error::AlreadyExists(_))
        ));

        node.blacklist().remove(first).await.unwrap();
        assert_eq!(node.blacklist().list().await.unwrap(), vec![second]);
        assert!(matches!(
            node.blacklist().remove(first).await,
            Err(Error::NotFound(_))
        ));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn blacklist_disconnects_and_blocks_peers() {
    common::timeout(async {
        let (_peer, peer_id, addr) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_blacklist()).await;
        assert!(node.blacklist().list().await.unwrap().is_empty());
        node.swarm().dial(addr.clone()).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());

        let mut events = node.swarm().listener().await.unwrap();
        node.blacklist().add(peer_id).await.unwrap();
        common::next_matching(&mut events, |event| {
            matches!(event, ConnexaSwarmEvent::ConnectionClosed { peer_id: id, num_established: 0, .. } if *id == peer_id)
        })
        .await;
        assert!(!node.swarm().is_connected(peer_id).await.unwrap());

        let Error::Dial(error) = node.swarm().dial(addr.clone()).await.unwrap_err() else {
            panic!("dial returns a connection error");
        };
        let DialError::Denied { cause } = error.as_ref() else {
            panic!("blacklist denies the connection");
        };
        assert!(cause.downcast_ref::<libp2p_allow_block_list::Blocked>().is_some());

        node.blacklist().remove(peer_id).await.unwrap();
        node.swarm().dial(addr).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn blacklist_rejects_inbound_connections() {
    common::timeout(async {
        let (peer, peer_id, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, addr) = common::spawn_connexa_with_config(|builder| {
            builder.with_blacklist_with_list([peer_id])
        })
        .await;
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
            panic!("blacklist denies the inbound connection");
        };
        assert!(cause.downcast_ref::<libp2p_allow_block_list::Blocked>().is_some());
        assert!(!node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_blacklist_returns_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let peer_id = PeerId::random();
        for result in [
            node.blacklist().add(peer_id).await,
            node.blacklist().remove(peer_id).await,
            node.blacklist().list().await.map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Other("blacklist")
                })
            ));
        }
    })
    .await;
}
