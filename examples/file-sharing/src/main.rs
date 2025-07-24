use clap::Parser;
use connexa::behaviour::request_response::RequestResponseConfig;
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, Protocol};
use futures::stream::FuturesUnordered;
use futures::{StreamExt, TryStreamExt};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

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

const FILE_SHARING_PROTOCOL: &str = "/connexa/file-share";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_request_response(vec![RequestResponseConfig {
            protocol: FILE_SHARING_PROTOCOL.to_string(),
            ..Default::default()
        }])
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

    match opt.argument {
        CliArgument::Provide { path, name } => {
            assert!(path.is_file());
            let mut listener = connexa
                .request_response()
                .listen_for_requests(&name)
                .await?;

            while let Some((peer_id, request_id, request)) = listener.next().await {
                assert!(path.is_file());
                let request_name = String::from_utf8_lossy(&request).to_string();
                if request_name != name {
                    continue;
                }
                let data = tokio::fs::read(&path).await?;

                connexa
                    .request_response()
                    .send_response(peer_id, request_id, data)
                    .await?;
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

            let response = connexa
                .request_response()
                .send_request(peer_id, (FILE_SHARING_PROTOCOL, name.clone()))
                .await?;
            tokio::io::stdout().write_all(&response).await?;
        }
    }

    Ok(())
}
