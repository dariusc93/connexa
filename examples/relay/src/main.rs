use clap::Parser;
use connexa::prelude::{DefaultConnexaBuilder, Multiaddr, PeerId, Protocol, identity::Keypair};
use std::io;
use std::net::Ipv4Addr;
use std::str::FromStr;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[clap(name = "relay-example")]
struct Opt {
    #[arg(long)]
    mode: Mode,

    #[arg(long)]
    seed: Option<u8>,

    #[arg(long)]
    port: Option<u16>,

    #[arg(long)]
    relay_address: Option<Multiaddr>,

    #[arg(long)]
    remote_peer_id: Option<PeerId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Parser)]
enum Mode {
    Dial,
    Listen,
    Server,
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(mode: &str) -> Result<Self, Self::Err> {
        match mode {
            "dial" => Ok(Mode::Dial),
            "listen" => Ok(Mode::Listen),
            "server" => Ok(Mode::Server),
            _ => Err("Expected either 'dial' or 'listen'".to_string()),
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
            .with_ping()
            .with_identify()
            .with_relay_server()
            .build()?,
        Mode::Dial | Mode::Listen => DefaultConnexaBuilder::with_existing_identity(&keypair)
            .enable_tcp()
            .enable_quic()
            .with_ping()
            .with_identify()
            .with_relay()
            .build()?,
    };

    let peer_id = keypair.public().to_peer_id();
    println!("Peer ID: {}", peer_id);

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

    if let Some(relay_address) = opt.relay_address.clone() {
        connexa.swarm().dial(relay_address).await?;
    }

    match opt.mode {
        Mode::Dial => {
            let relay_address = opt.relay_address.unwrap();
            let remote_peer_id = opt.remote_peer_id.unwrap();
            connexa
                .swarm()
                .dial(
                    relay_address
                        .with(Protocol::P2pCircuit)
                        .with(Protocol::P2p(remote_peer_id)),
                )
                .await?;

            if connexa.swarm().is_connected(remote_peer_id).await? {
                println!("Connected to {remote_peer_id}");
            }
        }
        Mode::Listen => {
            let relay_address = opt.relay_address.unwrap();
            connexa
                .swarm()
                .listen_on(relay_address.with(Protocol::P2pCircuit))
                .await?;

            let listening_addrs = connexa.swarm().listening_addresses().await?;

            assert!(!listening_addrs.is_empty());

            for addr in listening_addrs {
                println!("> Use {}", addr);
            }
        }
        Mode::Server => {
            let listening_addrs = connexa.swarm().listening_addresses().await?;

            assert!(!listening_addrs.is_empty());

            for addr in listening_addrs {
                let addr = addr.with(Protocol::P2p(peer_id));

                // We add our addresses as external addresses so that the relay behaviour will know what addresses are considered for the reservation.
                connexa.swarm().add_external_address(addr.clone()).await?;
                println!("> Use {}", addr.with(Protocol::P2pCircuit));
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
