use clap::Parser;
use connexa::prelude::{
    DefaultConnexaBuilder, GossipsubEvent, GossipsubMessage, Multiaddr, Protocol,
};
use futures::FutureExt;
use futures::StreamExt;
use rustyline_async::Readline;
use std::io::Write;

#[derive(Debug, Parser)]
#[clap(name = "gossipsub-example")]
struct Opt {
    #[clap(long)]
    topic: Option<String>,
    #[clap(long)]
    peers: Vec<Multiaddr>,
    #[clap(long)]
    listener: Vec<Multiaddr>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_gossipsub()
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

    let mut listener_ids = Vec::with_capacity(addrs.len());

    for addr in addrs {
        match connexa.swarm().listen_on(addr.clone()).await {
            Ok(id) => listener_ids.push(id),
            Err(e) => {
                println!("failed to listen on {addr}: {e}");
                continue;
            }
        };
    }

    let topic = opt.topic.unwrap_or_else(|| "test-net".to_string());

    connexa.gossipsub().subscribe(&topic).await?;

    let mut listener = connexa.gossipsub().listener(&topic).await?;

    for addr in opt.peers.iter() {
        if let Err(e) = connexa.swarm().dial(addr.clone()).await {
            println!("failed to dial {}: {}", addr, e);
        }
    }

    let peer_id = connexa.keypair().public().to_peer_id();

    let listen_addr = connexa.swarm().listening_addresses().await?;

    let (mut rl, mut stdout) =
        Readline::new(format!("{peer_id} >")).map_err(std::io::Error::other)?;

    for listener_id in listener_ids {
        if let Ok(addrs) = connexa.swarm().get_listening_addresses(listener_id).await {
            for addr in addrs {
                writeln!(
                    stdout,
                    "> listening on {}",
                    addr.with(Protocol::P2p(peer_id))
                )?;
            }
        }
    }

    loop {
        tokio::select! {
            Some(event) = listener.next() => {
                match event {
                    GossipsubEvent::Subscribed{ peer_id } => {
                        writeln!(stdout, "[{}] subscribed", peer_id)?;
                    },
                    GossipsubEvent::Unsubscribed{ peer_id } => {
                        writeln!(stdout, "[{}] unsubscribed", peer_id)?;
                    },
                    GossipsubEvent::Message{ message: GossipsubMessage {
                            source: Some(peer_id),
                            data,
                            ..
                        }
                    } => {
                        writeln!(stdout, "[{}] > {}", peer_id, String::from_utf8_lossy(&data))?;
                    }
                    _ => unreachable!()
                }
            },
            input = rl.readline().fuse() => match input {
                Ok(rustyline_async::ReadlineEvent::Line(line)) => {
                    if line.is_empty() {
                        continue;
                    }

                    let bytes = line.as_bytes().to_vec();

                    if let Err(e) = connexa.gossipsub().publish(&topic, bytes).await {
                        writeln!(stdout, "error publishing message: {e}")?;
                    }
                }
                Ok(rustyline_async::ReadlineEvent::Interrupted) => {
                    break;
                }
                Ok(rustyline_async::ReadlineEvent::Eof) => {
                    break;
                }
                Err(e) => {
                    writeln!(stdout, "error: {}", e)?;
                    break;
                }
            }
        }
    }

    Ok(())
}
