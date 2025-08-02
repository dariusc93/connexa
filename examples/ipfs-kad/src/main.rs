use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, PeerId};
use std::time::Duration;

const IPFS_PROTO_NAME: &str = "/ipfs/kad/1.0.0";

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
        .with_kademlia_with_config(IPFS_PROTO_NAME, |mut config| {
            config.set_query_timeout(Duration::from_secs(5 * 60));
            config
        })
        .build()?;

    for (addr, peer_id) in BOOTSTRAP_NODES {
        let peer_id: PeerId = peer_id.parse().expect("valid peer id");
        let addr: Multiaddr = addr.parse().expect("valid addr");
        connexa.dht().add_address(peer_id, addr).await?;
    }

    let infos = connexa.dht().find_peer(PeerId::random()).await?;

    for info in infos {
        println!("- PeerId {}", info.peer_id);
        println!("-- Addresses:");
        for addr in info.addrs {
            println!("--- {addr}")
        }
        println!();
    }

    Ok(())
}
