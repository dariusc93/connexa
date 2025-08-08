use connexa::prelude::DefaultConnexaBuilder;
use connexa::prelude::peer_store::memory::MemoryStore;
use connexa::prelude::swarm::dial_opts::DialOpts;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let connexa_a = DefaultConnexaBuilder::new_identity()
        .with_peer_store(MemoryStore::default())
        .enable_memory_transport()
        .build()?;

    let id_a = connexa_a
        .swarm()
        .listen_on("/memory/0".parse().unwrap())
        .await?;

    let peer_a_id = connexa_a.keypair().public().to_peer_id();

    let node_a_addrs = connexa_a.swarm().get_listening_addresses(id_a).await?;

    let connexa_b = DefaultConnexaBuilder::new_identity()
        .with_peer_store(MemoryStore::default())
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

    connexa_a.peer_store().remove_peer(peer_b_id).await?;
    connexa_b.peer_store().remove_peer(peer_a_id).await?;

    // connect to the other node and check to see if there been any entries into the peerstore.
    // but first, check to determine if the peer can be dialed as if the address has been in the peer store.

    connexa_a.swarm().dial(peer_b_id).await.unwrap_err();

    let opt = DialOpts::peer_id(peer_b_id).addresses(addrs_b).build();

    connexa_a.swarm().dial(opt).await.unwrap();

    // give the task time to process the spawn events. 
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

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
