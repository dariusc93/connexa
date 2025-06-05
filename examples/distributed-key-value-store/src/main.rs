use clap::Parser;
use connexa::prelude::dht::Quorum;
use connexa::prelude::{DHTEvent, DefaultConnexaBuilder, Multiaddr, PeerId, Protocol};
use futures::stream::BoxStream;
use futures::StreamExt;
use futures::{FutureExt, Stream};
use pollable_map::stream::StreamMap;
use rustyline_async::Readline;
use std::collections::HashSet;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug, Parser)]
#[clap(name = "distributed-key-value-store example")]
struct Opt {
    #[clap(long)]
    peer: Vec<Multiaddr>,
    #[clap(long)]
    listener: Vec<Multiaddr>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_kademlia()
        .start()?;

    let addrs = match opt.listener.is_empty() {
        true => vec![
            "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("valid multiaddr"),
            "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
        ],
        false => opt.listener,
    };

    for addr in addrs {
        if let Err(e) = connexa.swarm().listen_on(addr.clone()).await {
            println!("failed to listen on {}: {}", addr, e);
        }
    }

    tokio::task::yield_now().await;

    let addrs = connexa.swarm().listening_addresses().await?;

    let peer_id = connexa.keypair().public().to_peer_id();
    let (mut rl, mut stdout) =
        Readline::new(format!("{peer_id} >")).map_err(std::io::Error::other)?;

    for addr in addrs {
        let addr = addr.with(Protocol::P2p(peer_id));
        connexa.swarm().add_external_address(addr.clone()).await?;
        writeln!(stdout, "> listening on {}", addr)?;
    }

    for mut addr in opt.peer {
        let peer_id = addr
            .pop()
            .map(|protocol| {
                let Protocol::P2p(peer_id) = protocol else {
                    panic!("address does not contain a peer id");
                };
                peer_id
            })
            .expect("valid multiaddr with peer id");
        if let Err(e) = connexa.dht().add_address(peer_id, addr.clone()).await {
            writeln!(stdout, "failed to add DHT address {}: {}", addr, e)?;
        }
    }

    let mut listener = connexa.dht().listener(None::<String>).await?;

    let mut get_records_stream = StreamMap::default();
    let mut get_providers_stream = StreamMap::default();

    loop {
        tokio::select! {
            Some(event) = listener.next() => {
                match event {
                    DHTEvent::PutRecord{ .. } => {
                        writeln!(stdout, ">>> received a record")?;
                    }
                    DHTEvent::ProvideRecord{ .. } => {
                        writeln!(stdout, ">>> received a provider record")?;
                    }
                }
            },
            Some((key, result)) = get_records_stream.next() => {
                match result {
                    Ok(record) => {
                        writeln!(stdout, "record [{key}]: record {:?}", record)?;
                    },
                    Err(e) => {
                        writeln!(stdout, "recird [{key}]: failed to get record: {}", e)?;
                    }
                }
            }
            Some((key, result)) = get_providers_stream.next() => {
                match result {
                    Ok(record) => {
                        writeln!(stdout, "provider [{key}]: {:?}", record)?;
                    },
                    Err(e) => {
                        writeln!(stdout, "provider [{key}]: failed to get record: {}", e)?;
                    }
                }
            }
            input = rl.readline().fuse() => match input {
                Ok(rustyline_async::ReadlineEvent::Line(line)) => {
                    if line.is_empty() {
                        continue;
                    }

                    let mut cmd = line.split(' ');

                    match cmd.next() {
                        Some("GET") => {
                            let key = match cmd.next() {
                                Some(key) => key,
                                None => {
                                    writeln!(stdout, "GET: missing 'key' value")?;
                                    continue;
                                }
                            };
                            let st = match connexa.dht().get(key).await {
                                Ok(s) => s,
                                Err(e) => {
                                    writeln!(stdout, "failed to get {:?}: {}", key, e)?;
                                    continue;
                                }
                            };
                            get_records_stream.insert(key.to_string(), st);
                        },
                        Some("PUT") => {
                            let key = match cmd.next() {
                                Some(key) => key,
                                None => {
                                    writeln!(stdout, "PUT: missing 'key' value")?;
                                    continue;
                                }
                            };

                            let val = match cmd.next() {
                                Some(val) => val.as_bytes().to_vec(),
                                None => {
                                    writeln!(stdout, "PUT: missing 'value' value")?;
                                    continue;
                                }
                            };

                            if let Err(e) = connexa.dht().put(key, val, Quorum::One).await {
                                writeln!(stdout, "failed to put record: {}", e)?;
                                continue;
                            }

                            writeln!(stdout, "put record: {:?}", key)?;
                        },
                        Some("GET_PROVIDERS") => {
                            let key = match cmd.next() {
                                Some(key) => key,
                                None => {
                                    writeln!(stdout, "PUT: missing 'key' value")?;
                                    continue;
                                }
                            };

                            let st = match connexa.dht().get_providers(key).await {
                                Ok(st) => ProviderStream::new(st),
                                Err(e) => {
                                    writeln!(stdout, "failed to get providers fpr {key}: {}", e)?;
                                    continue;
                                }
                            };

                            get_providers_stream.insert(key.to_string(), st);
                        },
                        Some("PUT_PROVIDER") => {
                            let key = match cmd.next() {
                                Some(key) => key,
                                None => {
                                    writeln!(stdout, "PUT: missing 'key' value")?;
                                    continue;
                                }
                            };

                            if let Err(e) = connexa.dht().provide(key).await {
                                writeln!(stdout, "failed to provide key {key}: {}", e)?;
                                continue;
                            }
                            writeln!(stdout, "provide key: {}", key)?;

                        },
                        _ => {}
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

/// small wrapper that caches the providers and pass the difference on
struct ProviderStream {
    st: BoxStream<'static, std::io::Result<HashSet<PeerId>>>,
    cache: HashSet<PeerId>,
}

impl ProviderStream {
    fn new(st: BoxStream<'static, std::io::Result<HashSet<PeerId>>>) -> Self {
        Self {
            st,
            cache: HashSet::new(),
        }
    }
}

impl Stream for ProviderStream {
    type Item = std::io::Result<HashSet<PeerId>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match futures::ready!(self.st.poll_next_unpin(cx)) {
            Some(Ok(peers)) => {
                let diffs = peers
                    .difference(&self.cache)
                    .copied()
                    .collect::<HashSet<_>>();
                self.cache.extend(diffs.clone());
                Poll::Ready(Some(Ok(diffs)))
            }
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}
