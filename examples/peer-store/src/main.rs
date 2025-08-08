use connexa::prelude::DefaultConnexaBuilder;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let connexa_a = DefaultConnexaBuilder::new_identity()
        .with_peer_store()
        .enable_memory_transport()
        .build()?;

    let id_a = connexa_a
        .swarm()
        .listen_on("/memory/0".parse().unwrap())
        .await?;

    let peer_a_id = connexa_a.keypair().public().to_peer_id();

    let node_a_addrs = connexa_a.swarm().get_listening_addresses(id_a).await?;

    let connexa_b = DefaultConnexaBuilder::new_identity()
        .with_peer_store()
        .enable_memory_transport()
        .build()?;

    let id_b = connexa_b
        .swarm()
        .listen_on("/memory/0".parse().unwrap())
        .await?;

    let peer_b_id = connexa_b.keypair().public().to_peer_id();

    let node_b_addrs = connexa_b.swarm().get_listening_addresses(id_b).await?;

    for addr in node_a_addrs {
        connexa_b.peer_store().add_address(peer_a_id, addr).await?;
    }

    for addr in node_b_addrs {
        connexa_a.peer_store().add_address(peer_b_id, addr).await?;
    }

    let addrs_a = connexa_b.peer_store().list(peer_a_id).await?;
    let addrs_b = connexa_a.peer_store().list(peer_b_id).await?;

    println!(
        "Peer {} has addresses for {}: {:?}",
        peer_b_id,
        peer_a_id,
        addrs_a.iter().map(|a| a.to_string()).collect::<Vec<_>>()
    );

    println!(
        "Peer {} has addresses for {}: {:?}",
        peer_a_id,
        peer_b_id,
        addrs_b.iter().map(|a| a.to_string()).collect::<Vec<_>>()
    );

    Ok(())
}
