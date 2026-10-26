mod common;

use connexa::error::{Error, Protocol};
use connexa::prelude::{ConnexaSwarmEvent, Multiaddr, PeerId};
use std::io::ErrorKind;

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn manage_peer_addresses() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_peer_store()).await;
        let first = PeerId::random();
        let second = PeerId::random();
        let first_addr: Multiaddr = "/memory/1".parse().unwrap();
        let second_addr: Multiaddr = "/memory/2".parse().unwrap();
        assert!(node.peer_store().list_all().await.unwrap().is_empty());

        node.peer_store()
            .add_address(first, first_addr.clone())
            .await
            .unwrap();
        node.peer_store()
            .add_address(first, second_addr.clone())
            .await
            .unwrap();
        node.peer_store()
            .add_address(second, second_addr.clone())
            .await
            .unwrap();
        let addresses = node.peer_store().list(first).await.unwrap();
        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&first_addr));
        assert!(addresses.contains(&second_addr));
        let peers = node.peer_store().list_all().await.unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&(first, addresses)));
        assert!(peers.contains(&(second, vec![second_addr.clone()])));

        node.peer_store()
            .remove_address(first, first_addr)
            .await
            .unwrap();
        assert_eq!(
            node.peer_store().list(first).await.unwrap(),
            vec![second_addr.clone()]
        );
        assert_eq!(
            node.peer_store().remove_peer(first).await.unwrap(),
            vec![second_addr.clone()]
        );
        assert_eq!(
            node.peer_store().list_all().await.unwrap(),
            vec![(second, vec![second_addr.clone()])]
        );

        node.peer_store()
            .remove_address(second, second_addr)
            .await
            .unwrap();
        assert!(node.peer_store().list_all().await.unwrap().is_empty());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn duplicate_and_missing_addresses_return_errors() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_peer_store()).await;
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/memory/1".parse().unwrap();
        node.peer_store()
            .add_address(peer_id, addr.clone())
            .await
            .unwrap();
        let error = node
            .peer_store()
            .add_address(peer_id, addr.clone())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Io(error) if error.kind() == ErrorKind::AlreadyExists));

        let missing_addr = "/memory/2".parse().unwrap();
        let error = node
            .peer_store()
            .remove_address(peer_id, missing_addr)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Io(error) if error.kind() == ErrorKind::NotFound));
        assert_eq!(
            node.peer_store().list(peer_id).await.unwrap(),
            vec![addr.clone()]
        );
        node.peer_store().remove_peer(peer_id).await.unwrap();

        for result in [
            node.peer_store().list(peer_id).await.map(|_| ()),
            node.peer_store().remove_peer(peer_id).await.map(|_| ()),
            node.peer_store().remove_address(peer_id, addr).await,
        ] {
            assert!(matches!(result, Err(Error::Io(error)) if error.kind() == ErrorKind::NotFound));
        }
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn stored_address_allows_dialing_by_peer_id() {
    common::timeout(async {
        let (_peer, peer_id, addr) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_peer_store()).await;
        let Error::Dial(error) = node.swarm().dial(peer_id).await.unwrap_err() else {
            panic!("dial returns a connection error");
        };
        assert!(matches!(
            error.as_ref(),
            connexa::prelude::swarm::DialError::NoAddresses
        ));

        node.peer_store().add_address(peer_id, addr).await.unwrap();
        node.swarm().dial(peer_id).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn connected_peer_address_is_stored() {
    common::timeout(async {
        let (_peer, peer_id, addr) = common::spawn_connexa_with_config(|builder| builder).await;
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_peer_store()).await;
        node.swarm().dial(addr.clone()).await.unwrap();
        assert!(node.peer_store().list(peer_id).await.unwrap().contains(&addr));

        let mut events = node.swarm().listener().await.unwrap();
        node.swarm().disconnect(peer_id).await.unwrap();
        common::next_matching(&mut events, |event| {
            matches!(event, ConnexaSwarmEvent::ConnectionClosed { peer_id: id, num_established: 0, .. } if *id == peer_id)
        })
        .await;
        assert!(node.peer_store().list(peer_id).await.unwrap().contains(&addr));
        node.swarm().dial(peer_id).await.unwrap();
        assert!(node.swarm().is_connected(peer_id).await.unwrap());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_peer_store_returns_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/memory/1".parse().unwrap();
        for result in [
            node.peer_store().add_address(peer_id, addr.clone()).await,
            node.peer_store().remove_address(peer_id, addr).await,
            node.peer_store().remove_peer(peer_id).await.map(|_| ()),
            node.peer_store().list(peer_id).await.map(|_| ()),
            node.peer_store().list_all().await.map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Other("peerstore")
                })
            ));
        }
    })
    .await;
}
