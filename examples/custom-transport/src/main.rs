use connexa::handle::Connexa;
use connexa::prelude::transport::muxing::StreamMuxerBox;
use connexa::prelude::transport::transport::{MemoryTransport, Transport};
use connexa::prelude::transport::upgrade::Version;
use connexa::prelude::transport::{noise, yamux};
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, Protocol};
use std::io;
use tracing_subscriber::EnvFilter;

async fn create_node(port: impl Into<Option<u64>>) -> io::Result<Connexa> {
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
        .start()?;

    let port = port.into().unwrap_or_default();

    connexa
        .swarm()
        .listen_on(Multiaddr::empty().with(Protocol::Memory(port)))
        .await?;

    Ok(connexa)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let (node_a, node_b) = tokio::try_join!(create_node(4410), create_node(4411))?;

    let addr_a = node_a
        .swarm()
        .listening_addresses()
        .await?
        .first()
        .cloned()
        .unwrap();
    println!("A - Listening on: {}", addr_a);

    let addr_b = node_b
        .swarm()
        .listening_addresses()
        .await?
        .first()
        .cloned()
        .unwrap();
    println!("B - Listening on: {}", addr_b);

    node_a.swarm().dial(addr_b).await?;

    Ok(())
}
