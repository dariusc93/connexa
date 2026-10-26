#![allow(dead_code)]

use std::future::Future;
use std::time::Duration;

use connexa::builder::IntoKeypair;
use connexa::handle::Connexa;
use connexa::prelude::identity::Keypair;
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr};
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};
use futures_timeout::TimeoutExt;
use libp2p::PeerId;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn spawn_connexa(keypair: impl IntoKeypair) -> std::io::Result<Connexa> {
    let builder = DefaultConnexaBuilder::with_existing_identity(keypair)?
        .enable_memory_transport()
        .with_blacklist();

    #[cfg(feature = "gossipsub")]
    let builder = builder.with_gossipsub();
    #[cfg(feature = "identify")]
    let builder = builder.with_identify();
    #[cfg(feature = "kad")]
    let builder = builder.with_kademlia_with_config("/ipfs/kad/1.0.0", |mut config| {
        config.set_record_filtering(libp2p::kad::StoreInserts::FilterBoth);
        config
    });
    #[cfg(feature = "request-response")]
    let builder = builder.with_request_response(vec![]);
    #[cfg(feature = "autonat")]
    let builder = builder.with_autonat_v1();
    #[cfg(feature = "ping")]
    let builder = builder.with_ping();

    #[cfg(all(feature = "relay", not(target_arch = "wasm32")))]
    let builder = builder.with_relay().with_relay_server();

    let (connexa, _, _) = spawn_listening_connexa(builder).await?;
    Ok(connexa)
}

pub async fn spawn_connexa_with_config(
    configure: impl FnOnce(DefaultConnexaBuilder) -> DefaultConnexaBuilder,
) -> (Connexa, PeerId, Multiaddr) {
    let builder = DefaultConnexaBuilder::with_existing_identity(Keypair::generate_ed25519())
        .unwrap()
        .enable_memory_transport()
        .set_swarm_config(|config| config.with_idle_connection_timeout(DEFAULT_TIMEOUT));

    spawn_listening_connexa(configure(builder)).await.unwrap()
}

async fn spawn_listening_connexa(
    builder: DefaultConnexaBuilder,
) -> std::io::Result<(Connexa, PeerId, Multiaddr)> {
    let connexa = builder.build().await?;

    let listen_id = connexa.swarm().listen_on("/memory/0").await?;

    let peer_id = connexa.keypair().public().to_peer_id();
    let addrs = connexa.swarm().get_listening_addresses(listen_id).await?;
    let addr = addrs.first().cloned().expect("multiaddr available");
    let addr = addr.with_p2p(peer_id).unwrap();

    connexa.swarm().add_external_address(addr.clone()).await?;

    Ok((connexa, peer_id, addr))
}

pub async fn spawn_connexa_with_default_key() -> Connexa {
    let [(node, _, _)] = spawn_connexa_nodes_with_default_keys::<1>().await;
    node
}

pub async fn spawn_connexa_nodes<const N: usize>(
    keypairs: impl IntoIterator<Item = impl IntoKeypair>,
) -> std::io::Result<[(Connexa, PeerId, Multiaddr); N]> {
    let nodes = FuturesUnordered::from_iter(keypairs.into_iter().map(|kp| spawn_connexa(kp)))
        .try_filter_map(|connexa| async move {
            let peer_id = connexa.keypair().public().to_peer_id();
            let addrs = connexa.swarm().listening_addresses().await?;
            assert!(!addrs.is_empty());
            assert_eq!(addrs.len(), 1);
            let addr = addrs.first().cloned().expect("multiaddr available");
            let addr = addr.with_p2p(peer_id).unwrap();
            Ok(Some((connexa, peer_id, addr)))
        })
        .try_collect::<Vec<_>>()
        .and_then(|nodes| async move {
            let nodes: [_; N] = nodes.try_into().expect("array length is correct");
            Ok(nodes)
        })
        .await?;

    Ok(nodes)
}

pub async fn spawn_connexa_nodes_with_default_keys<const N: usize>()
-> [(Connexa, PeerId, Multiaddr); N] {
    spawn_connexa_nodes((0..N).map(|_| Keypair::generate_ed25519()))
        .await
        .unwrap()
}

pub async fn timeout<T>(future: impl Future<Output = T>) -> T {
    future
        .timeout(DEFAULT_TIMEOUT)
        .await
        .expect("operation completes in time")
}

pub async fn next_matching<S>(stream: &mut S, predicate: impl Fn(&S::Item) -> bool) -> S::Item
where
    S: Stream + Unpin,
{
    while let Some(event) = stream.next().await {
        if predicate(&event) {
            return event;
        }
    }
    panic!("event stream remains open");
}
