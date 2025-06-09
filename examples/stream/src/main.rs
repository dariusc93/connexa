use clap::Parser;
use connexa::handle::Connexa;
use connexa::prelude::{
    DefaultConnexaBuilder, Multiaddr, PeerId, Protocol, Stream, StreamProtocol,
};
use futures::StreamExt;
use futures::{AsyncReadExt, AsyncWriteExt};
use rand::RngCore;
use std::io;
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Parser)]
#[clap(name = "stream-example")]
struct Opt {
    #[clap(long)]
    data_size: Option<usize>,
    #[clap(long)]
    peer: Option<Multiaddr>,
    #[clap(long)]
    protocol: Option<StProtocol>,
    #[clap(long)]
    listen_addr: Vec<Multiaddr>,
}

#[derive(Debug, Clone)]
struct StProtocol(StreamProtocol);

impl From<StProtocol> for StreamProtocol {
    fn from(p: StProtocol) -> Self {
        p.0
    }
}

impl Deref for StProtocol {
    type Target = StreamProtocol;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for StProtocol {
    type Err = io::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let str = StreamProtocol::try_from_owned(s.to_string()).map_err(io::Error::other)?;
        Ok(StProtocol(str))
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::init();
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_streams()
        .start()?;

    let addrs = match opt.listen_addr.is_empty() {
        true => vec![
            "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("valid multiaddr"),
            "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
        ],
        false => opt.listen_addr,
    };

    for addr in addrs {
        if let Err(e) = connexa.swarm().listen_on(addr.clone()).await {
            println!("failed to listen on {}: {}", addr, e);
        }
    }

    let addrs = connexa.swarm().listening_addresses().await?;

    let peer_id = connexa.keypair().public().to_peer_id();

    for addr in addrs {
        let addr = addr.with(Protocol::P2p(peer_id));
        println!("new address - {}", addr);
    }

    let protocol = opt
        .protocol
        .map(StreamProtocol::from)
        .unwrap_or_else(|| StreamProtocol::new("/echo/1.0.0"));

    let data_size = opt.data_size.unwrap_or(256);

    let mut incoming_streams = connexa.stream().new_stream(protocol.clone()).await?;

    tokio::spawn(async move {
        while let Some((peer, stream)) = incoming_streams.next().await {
            match echo(data_size, stream).await {
                Ok(n) => {
                    tracing::info!(%peer, "Echoed {n} bytes!");
                }
                Err(e) => {
                    tracing::warn!(%peer, "Echo failed: {e}");
                    continue;
                }
            };
        }
    });

    if let Some(address) = opt.peer {
        let Protocol::P2p(peer_id) = address.clone().pop().expect("valid multiaddr") else {
            panic!("Expected peer id");
        };

        connexa.swarm().dial(address).await?;
        let c = connexa.clone();
        let protocol = protocol.clone();
        tokio::spawn(connection_handler(protocol, peer_id, c));
    }

    tokio::signal::ctrl_c().await?;

    Ok(())
}

async fn connection_handler(protocol: StreamProtocol, peer: PeerId, connexa: Connexa) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let stream = match connexa.stream().open_stream(peer, protocol.clone()).await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::error!(%peer, %error);
                continue;
            }
        };

        if let Err(e) = send(stream).await {
            tracing::warn!(%peer, "Echo protocol failed: {e}");
            continue;
        }

        tracing::info!(%peer, "Echo complete!")
    }
}

async fn echo(size: usize, mut stream: Stream) -> std::io::Result<usize> {
    let mut total = 0;

    let mut buf = vec![0u8; size];

    loop {
        let read = stream.read(&mut buf).await?;
        if read == 0 {
            return Ok(total);
        }

        total += read;
        stream.write_all(&buf[..read]).await?;
    }
}

async fn send(mut stream: Stream) -> io::Result<()> {
    let num_bytes = rand::random::<usize>() % 1000;

    let mut bytes = vec![0; num_bytes];
    rand::thread_rng().fill_bytes(&mut bytes);

    stream.write_all(&bytes).await?;

    let mut buf = vec![0; num_bytes];
    stream.read_exact(&mut buf).await?;

    if bytes != buf {
        return Err(io::Error::other("incorrect echo"));
    }

    stream.close().await?;

    Ok(())
}
