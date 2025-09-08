use connexa::prelude::swarm::SwarmEvent;
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, PeerId};

pub const BOOTSTRAP_NODES: &[(&str, &str)] = &[
    (
        "/ip4/104.131.131.82/tcp/4001",
        "QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    ),
];

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .enable_dns()
        .with_relay()
        .with_autorelay()
        .with_ping()
        .with_identify()
        .with_kademlia()
        .set_swarm_event_callback(|_swarm, event, ()| {
            if matches!(
                event,
                SwarmEvent::NewListenAddr { .. }
                    | SwarmEvent::ListenerError { .. }
                    | SwarmEvent::ListenerClosed { .. }
            ) {
                println!("{event:?}");
            }
        })
        .build()?;

    for (addr, peer_id) in BOOTSTRAP_NODES {
        let peer_id: PeerId = peer_id.parse().expect("valid peer id");
        let addr: Multiaddr = addr.parse().expect("valid addr");
        connexa.dht().add_address(peer_id, addr).await?;
    }

    connexa.dht().bootstrap().await?;

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    Ok(())
}
