use clap::Parser;
use connexa::prelude::dial_opts::DialOpts;
use connexa::prelude::{DefaultConnexaBuilder, Keypair, Multiaddr, PeerId, Protocol};
use futures::future::FutureExt;
use rustyline_async::Readline;
use std::io;
use std::io::Write;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[clap(name = "rendezvous-example")]
struct Opt {
    #[arg(long)]
    mode: Mode,

    #[arg(long)]
    seed: Option<u8>,

    #[arg(long)]
    port: Option<u16>,

    #[arg(long)]
    rendezvous_node: Vec<Multiaddr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Parser)]
enum Mode {
    Client,
    Server,
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(mode: &str) -> Result<Self, Self::Err> {
        match mode {
            "client" => Ok(Mode::Client),
            "server" => Ok(Mode::Server),
            _ => Err("Expected either 'client' or 'server'".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
    let opt = Opt::parse();
    let keypair = generate_ed25519(opt.seed);
    let connexa = match opt.mode {
        Mode::Server => DefaultConnexaBuilder::with_existing_identity(&keypair)
            .enable_tcp()
            .enable_quic()
            .with_rendezvous_server()
            .set_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(60)))
            .build()?,
        Mode::Client => DefaultConnexaBuilder::with_existing_identity(&keypair)
            .enable_tcp()
            .enable_quic()
            .with_rendezvous_client()
            .set_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(60)))
            .build()?,
    };

    let base_addr = Multiaddr::empty().with(Protocol::Ip4(Ipv4Addr::new(0, 0, 0, 0)));

    connexa
        .swarm()
        .listen_on(base_addr.clone().with(Protocol::Tcp(opt.port.unwrap_or(0))))
        .await?;

    connexa
        .swarm()
        .listen_on(
            base_addr
                .with(Protocol::Udp(opt.port.unwrap_or(0)))
                .with(Protocol::QuicV1),
        )
        .await?;

    let peer_id = connexa.keypair().public().to_peer_id();

    let listen_addr = connexa.swarm().listening_addresses().await?;

    for addr in listen_addr {
        let addr = addr.with(Protocol::P2p(peer_id));
        if let Err(e) = connexa.swarm().add_external_address(addr.clone()).await {
            println!("failed to add external address: {}", e);
            continue;
        }

        println!(">> listening on {}", addr);
    }

    for mut addr in opt.rendezvous_node {
        let peer_id = match addr.pop() {
            Some(Protocol::P2p(peer_id)) => peer_id,
            _ => {
                println!("invalid rendezvous node address: {}", addr);
                continue;
            }
        };

        if let Err(e) = connexa
            .swarm()
            .add_peer_address(peer_id, addr.clone())
            .await
        {
            println!("failed to add rendezvous node: {}", e);
            continue;
        }

        let opt = DialOpts::peer_id(peer_id).addresses(vec![addr]).build();
        if let Err(e) = connexa.swarm().dial(opt).await {
            println!("failed to dial rendezvous node: {}", e);
            continue;
        }
    }

    if let Mode::Client = opt.mode {
        let (mut rl, mut stdout) =
            Readline::new(format!("{peer_id} >")).map_err(std::io::Error::other)?;

        loop {
            tokio::select! {
                input = rl.readline().fuse() => match input {
                    Ok(rustyline_async::ReadlineEvent::Line(line)) => {
                        if line.is_empty() {
                            continue;
                        }

                        let mut cmd = line.split(' ');

                        match cmd.next() {
                            Some("register") => {
                                let peer_id = match cmd.next() {
                                    Some(peer) => match PeerId::from_str(peer) {
                                        Ok(peer) => peer,
                                        Err(e) => {
                                            writeln!(stdout, "register: invalid peer: {e}")?;
                                            continue;
                                        }
                                    },
                                    None => {
                                        writeln!(stdout, "register: missing 'peer`")?;
                                        continue;
                                    }
                                };

                                let namespace = match cmd.next() {
                                    Some(namespace) => namespace.to_string(),
                                    None => {
                                        writeln!(stdout, "register: missing 'namespace`")?;
                                        continue;
                                    }
                                };

                                if let Err(e) = connexa.rendezvous().register(peer_id, namespace.clone(), None).await {
                                    writeln!(stdout, "unable to register: {e}")?;
                                    continue;
                                }

                                writeln!(stdout, "registered with {peer_id} under namespace \"{namespace}\"")?;
                            },
                            Some("unregister") => {
                                let peer_id = match cmd.next() {
                                    Some(peer) => match PeerId::from_str(peer) {
                                        Ok(peer) => peer,
                                        Err(e) => {
                                            writeln!(stdout, "unregister: invalid peer: {e}")?;
                                            continue;
                                        }
                                    },
                                    None => {
                                        writeln!(stdout, "unregister: missing 'peer`")?;
                                        continue;
                                    }
                                };

                                let namespace = match cmd.next() {
                                    Some(namespace) => namespace.to_string(),
                                    None => {
                                        writeln!(stdout, "unregister: missing 'namespace`")?;
                                        continue;
                                    }
                                };

                                if let Err(e) = connexa.rendezvous().unregister(peer_id, namespace.clone()).await {
                                    writeln!(stdout, "unable to unregister: {e}")?;
                                    continue;
                                }

                                writeln!(stdout, "unregistered with {peer_id} under namespace \"{namespace}\"")?;
                            },
                            Some("discover") => {
                                let peer_id = match cmd.next() {
                                    Some(peer) => match PeerId::from_str(peer) {
                                        Ok(peer) => peer,
                                        Err(e) => {
                                            writeln!(stdout, "discover: invalid peer: {e}")?;
                                            continue;
                                        }
                                    },
                                    None => {
                                        writeln!(stdout, "discover: missing 'peer`")?;
                                        continue;
                                    }
                                };

                                let namespace = cmd.next().map(ToOwned::to_owned);

                                let (_cookie, list) = match connexa.rendezvous().discovery(peer_id, namespace, None, None).await {
                                    Ok(list) => list,
                                    Err(e) => {
                                        writeln!(stdout, "unable to discover peers on node {peer_id}: {e}")?;
                                        continue;
                                    }
                                };

                                for (peer_id, addrs) in list {
                                    for addr in addrs {
                                        writeln!(stdout, ">> discovered {peer_id}: {addr}")?;
                                    }
                                }
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
    }

    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn generate_ed25519(secret_key_seed: Option<u8>) -> Keypair {
    match secret_key_seed {
        Some(seed) => {
            let mut bytes = [0u8; 32];
            bytes[0] = seed;

            Keypair::ed25519_from_bytes(bytes).expect("only errors on wrong length")
        }
        None => Keypair::generate_ed25519(),
    }
}
