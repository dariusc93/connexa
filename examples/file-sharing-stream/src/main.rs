use clap::Parser;
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, Protocol, Stream, StreamProtocol};
use futures::stream::FuturesUnordered;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use std::path::{Path, PathBuf};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use unsigned_varint::aio;

const PROTOCOL: StreamProtocol = StreamProtocol::new("/connexa/file-share");

#[derive(Debug, Parser)]
#[clap(name = "file-sharing-example")]
struct Opt {
    #[clap(long)]
    peer: Option<Multiaddr>,
    #[clap(long)]
    listener: Vec<Multiaddr>,
    #[command(subcommand)]
    argument: CliArgument,
}

#[derive(Debug, Parser)]
enum CliArgument {
    Provide {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        name: String,
    },
    Get {
        #[arg(long)]
        name: String,
    },
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_streams()
        .build()?;

    let addrs = match opt.listener.is_empty() {
        true => vec![
            "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("valid multiaddr"),
            "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
        ],
        false => opt.listener,
    };

    let ids = FuturesUnordered::from_iter(
        addrs
            .iter()
            .cloned()
            .map(|addr| async { (addr.clone(), connexa.swarm().listen_on(addr).await) }),
    )
    .filter_map(|(addr, result)| async move {
        result
            .map_err(|e| {
                println!("failed to listen on {addr}: {e}");
                e
            })
            .ok()
    })
    .collect::<Vec<_>>()
    .await;

    let peer_id = connexa.keypair().public().to_peer_id();

    for id in ids {
        let addrs = match connexa.swarm().get_listening_addresses(id).await {
            Ok(addrs) => addrs,
            Err(e) => {
                println!("failed to obtain listening addresses for {id}: {e}");
                continue;
            }
        };
        for addr in addrs {
            let addr = addr.with(Protocol::P2p(peer_id));
            connexa.swarm().add_external_address(addr.clone()).await?;
            println!("new address - {addr}");
        }
    }

    let mut incoming_streams = connexa.stream().new_stream(PROTOCOL).await?;

    match opt.argument {
        CliArgument::Provide { path, name } => {
            assert!(path.is_file());
            while let Some((peer, stream)) = incoming_streams.next().await {
                let path = path.clone();
                let name = name.clone();
                tokio::spawn(async move {
                    println!("new connection from {peer}");
                    if let Err(e) = provide_file(&path, &name, stream).await {
                        println!("failed to provide file: {e}");
                    }
                });
            }
        }
        CliArgument::Get { name } => {
            let addr = opt.peer.expect("address is required");
            let peer_id = addr
                .iter()
                .last()
                .map(|protocol| {
                    let Protocol::P2p(peer_id) = protocol else {
                        panic!("address does not contain a peer id");
                    };
                    peer_id
                })
                .expect("valid multiaddr");

            connexa.swarm().dial(addr).await?;

            let stream = connexa.stream().open_stream(peer_id, PROTOCOL).await?;

            if let Err(e) = request_file(&name, stream).await {
                println!("failed to request file: {e}");
            }
        }
    }

    Ok(())
}

async fn request_file(file: &str, mut stream: Stream) -> std::io::Result<()> {
    let mut buf = unsigned_varint::encode::usize_buffer();
    let file_len = unsigned_varint::encode::usize(file.len(), &mut buf);
    stream.write_all(file_len).await?;
    stream.write_all(file.as_bytes()).await?;

    let mut stdout = tokio::io::stdout().compat_write();

    futures::io::copy(&mut stream, &mut stdout).await?;

    stream.close().await?;

    Ok(())
}

async fn provide_file(
    path: impl AsRef<Path>,
    name: &str,
    mut stream: Stream,
) -> std::io::Result<()> {
    let filename_size = aio::read_usize(&mut stream)
        .await
        .map_err(std::io::Error::other)?;

    let mut filename_bytes = vec![0u8; filename_size];

    stream.read_exact(&mut filename_bytes).await?;

    let filename = String::from_utf8_lossy(&filename_bytes);

    if name != filename {
        stream.close().await?;
        return Ok(());
    }

    let file = tokio::fs::File::open(path).await?;
    let mut file = file.compat();

    futures::io::copy(&mut file, &mut stream).await?;

    stream.close().await?;

    Ok(())
}
