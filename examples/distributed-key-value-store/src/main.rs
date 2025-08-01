use clap::Parser;
use connexa::prelude::dht::{ProviderRecord, Quorum, Record, StoreInserts};
use connexa::prelude::{
    DHTEvent, DefaultConnexaBuilder, Multiaddr, PeerId, Protocol, RecordHandle,
};
use futures::StreamExt;
use futures::stream::{BoxStream, FuturesUnordered};
use futures::{FutureExt, Stream};
use pollable_map::stream::StreamMap;
use rustyline_async::Readline;
use std::collections::HashSet;
use std::io;
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
    #[clap(long)]
    enable_filtering: bool,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let opt = Opt::parse();
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .enable_quic()
        .with_kademlia_with_config("/ipfs/kad/1.0.0", move |mut config| {
            if opt.enable_filtering {
                config.set_record_filtering(StoreInserts::FilterBoth);
            }
            config
        })
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

    let peer_id = connexa.keypair().public().to_peer_id();
    let (mut rl, mut stdout) =
        Readline::new(format!("{peer_id} >")).map_err(std::io::Error::other)?;

    let ids = FuturesUnordered::from_iter(
        addrs
            .iter()
            .cloned()
            .map(|addr| async { (addr.clone(), connexa.swarm().listen_on(addr).await) }),
    )
    .filter_map(|(addr, result)| {
        let mut stdout = stdout.clone();
        async move {
            result
                .map_err(|e| {
                    let _ = writeln!(stdout, "> failed to listen on {addr}: {e}");
                    e
                })
                .ok()
        }
    })
    .collect::<Vec<_>>()
    .await;

    let peer_id = connexa.keypair().public().to_peer_id();

    for id in ids {
        let addrs = match connexa.swarm().get_listening_addresses(id).await {
            Ok(addrs) => addrs,
            Err(e) => {
                writeln!(
                    stdout,
                    "> failed to obtain listening addresses for {id}: {e}"
                )?;
                continue;
            }
        };
        for addr in addrs {
            let addr = addr.with(Protocol::P2p(peer_id));
            connexa.swarm().add_external_address(addr.clone()).await?;
            writeln!(stdout, "> listening on {addr}")?;
        }
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
            writeln!(stdout, "> failed to add DHT address {addr}: {e}")?;
        }
    }

    let mut listener = connexa.dht().listener(None::<String>).await?;

    let mut get_records_stream = StreamMap::default();
    let mut get_providers_stream = StreamMap::default();

    loop {
        tokio::select! {
            Some(event) = listener.next() => {
                match event {
                    DHTEvent::PutRecord { source, record } => {
                        writeln!(stdout, ">>> received a record from {}", source)?;
                        record_to_writer(&mut stdout, record)?;
                    }
                    DHTEvent::ProvideRecord { record } => {
                        writeln!(stdout, ">>> received a provider record")?;
                        provider_record_to_writer(&mut stdout, record)?;
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

fn provider_record_to_writer<W: Write>(
    writer: &mut W,
    record: RecordHandle<ProviderRecord>,
) -> io::Result<()> {
    if let RecordHandle {
        record: Some(record),
        confirm: Some(ch),
    } = record
    {
        writeln!(
            writer,
            ">>> record key: {}",
            String::from_utf8_lossy(record.key.as_ref())
        )?;
        writeln!(writer, ">>> record provider: {}", record.provider)?;
        writeln!(
            writer,
            ">>> record provider address: {:?}",
            record.addresses
        )?;
        let _ = ch.send(Ok(record));
    }
    Ok(())
}

fn record_to_writer<W: Write>(writer: &mut W, record: RecordHandle<Record>) -> io::Result<()> {
    if let RecordHandle {
        record: Some(record),
        confirm: Some(ch),
    } = record
    {
        writeln!(
            writer,
            ">>> record key: {}",
            String::from_utf8_lossy(record.key.as_ref())
        )?;
        writeln!(
            writer,
            ">>> record value: {:?}",
            String::from_utf8_lossy(record.value.as_ref())
        )?;
        if let Some(publisher) = record.publisher {
            writeln!(writer, ">>> record publisher: {}", publisher)?;
        }
        let _ = ch.send(Ok(record));
    }
    Ok(())
}
