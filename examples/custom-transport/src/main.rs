use connexa::handle::Connexa;
use connexa::prelude::transport::muxing::StreamMuxerBox;
use connexa::prelude::transport::transport::{MemoryTransport, Transport};
use connexa::prelude::transport::upgrade::Version;
use connexa::prelude::transport::{noise, yamux};
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, Protocol};
use std::io;
use tracing_subscriber::EnvFilter;

async fn create_node(port: impl Into<Option<u64>>) -> io::Result<(Connexa, Multiaddr)> {
    let connexa = DefaultConnexaBuilder::new_identity()
        .with_ping()
        .with_custom_transport(move |keypair| {
            let auth_upgrade = noise::Config::new(keypair).map_err(std::io::Error::other)?;
            let multiplex_upgrade = yamux::Config::default();
            let memory_transport = MemoryTransport::new();
            let transport = memory_transport
                .upgrade(Version::V1)
                .authenticate(auth_upgrade)
                .multiplex(multiplex_upgrade)
                .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
                .boxed();
            Ok(transport)
        })?
        .build()?;

    let port = port.into().unwrap_or_default();

    let id = connexa
        .swarm()
        .listen_on(Multiaddr::empty().with(Protocol::Memory(port)))
        .await?;

    let addrs = connexa.swarm().get_listening_addresses(id).await?;

    let addr = addrs.first().cloned().expect("valid");

    Ok((connexa, addr))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let ((node_a, addr_a), (node_b, addr_b)) =
        tokio::try_join!(create_node(None), create_node(None))?;

    println!("A - Listening on: {addr_a}");

    println!("B - Listening on: {addr_a}");

    node_a.swarm().dial(addr_b).await?;

    let is_connected = node_b
        .swarm()
        .is_connected(node_a.keypair().public().to_peer_id())
        .await?;

    println!("B: Is connected to A: {is_connected}");

    Ok(())
}
