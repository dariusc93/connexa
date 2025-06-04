pub mod dummy;
pub mod request_response;
mod rr_man;

use either::Either;
use libp2p::dcutr::Behaviour as Dcutr;
use libp2p::identify::Behaviour as Identify;
use libp2p::kad::Behaviour as Kademlia;
use libp2p::kad::store::MemoryStore;
use libp2p::mdns::tokio::Behaviour as Mdns;
use libp2p::ping::Behaviour as Ping;
use libp2p::relay::client::Behaviour as RelayClient;
use libp2p::relay::{Behaviour as RelayServer, client};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{StreamProtocol, autonat};

use crate::builder::{Config, Protocols};
use indexmap::IndexMap;
use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p_allow_block_list::BlockedPeers;
use rand::rngs::OsRng;
use std::fmt::Debug;

#[derive(NetworkBehaviour)]
pub struct Behaviour<C>
where
    C: NetworkBehaviour,
    <C as NetworkBehaviour>::ToSwarm: Debug + Send,
{
    // connection management
    pub block_list: libp2p_allow_block_list::Behaviour<BlockedPeers>,
    pub connection_limits: Toggle<libp2p_connection_limits::Behaviour>,

    // networking
    pub relay: Toggle<RelayServer>,
    pub relay_client: Toggle<RelayClient>,

    #[cfg(not(target_arch = "wasm32"))]
    pub upnp: Toggle<libp2p::upnp::tokio::Behaviour>,
    #[cfg(not(target_arch = "wasm32"))]
    pub dcutr: Toggle<Dcutr>,

    // discovery
    pub rendezvous_client: Toggle<libp2p::rendezvous::client::Behaviour>,
    pub rendezvous_server: Toggle<libp2p::rendezvous::server::Behaviour>,
    #[cfg(not(target_arch = "wasm32"))]
    pub mdns: Toggle<Mdns>,
    pub kademlia: Toggle<Kademlia<MemoryStore>>,

    pub identify: Toggle<Identify>,
    pub pubsub: Either<Toggle<libp2p::gossipsub::Behaviour>, Toggle<libp2p::floodsub::Floodsub>>,
    pub ping: Toggle<Ping>,
    #[cfg(feature = "stream")]
    pub stream: Toggle<libp2p_stream::Behaviour>,

    pub autonat_v1: Toggle<autonat::v1::Behaviour>,
    pub autonat_v2_client: Toggle<autonat::v2::client::Behaviour>,
    pub autonat_v2_server: Toggle<autonat::v2::server::Behaviour>,

    // TODO: Write a macro or behaviour to support multiple request-response behaviour
    pub rr_man: Toggle<rr_man::Behaviour>,
    pub rr_1: Toggle<request_response::Behaviour>,
    pub rr_2: Toggle<request_response::Behaviour>,
    pub rr_3: Toggle<request_response::Behaviour>,
    pub rr_4: Toggle<request_response::Behaviour>,
    pub rr_5: Toggle<request_response::Behaviour>,
    pub rr_6: Toggle<request_response::Behaviour>,
    pub rr_7: Toggle<request_response::Behaviour>,
    pub rr_8: Toggle<request_response::Behaviour>,
    pub rr_9: Toggle<request_response::Behaviour>,
    pub rr_0: Toggle<request_response::Behaviour>,

    // custom behaviour
    pub custom: Toggle<C>,
}

impl<C> Behaviour<C>
where
    C: NetworkBehaviour,
    <C as NetworkBehaviour>::ToSwarm: Debug + Send,
{
    pub(crate) fn new(
        keypair: &Keypair,
        custom_behaviour: Option<C>,
        config: Config,
        protocols: Protocols,
    ) -> std::io::Result<(Self, Option<client::Transport>)> {
        let peer_id = keypair.public().to_peer_id();

        tracing::info!("net: starting with peer id {}", peer_id);

        #[cfg(not(target_arch = "wasm32"))]
        let mdns = protocols
            .mdns
            .then(|| Mdns::new(Default::default(), peer_id))
            .transpose()?
            .into();

        let store = { MemoryStore::with_config(peer_id, Default::default()) };

        let kad_config = config
            .kademlia_config
            .map(|(protocol, config_cb)| {
                let protocol = StreamProtocol::try_from_owned(protocol).expect("valid protocol");
                let config = libp2p::kad::Config::new(protocol);
                config_cb(config)
            })
            .unwrap_or_else(|| libp2p::kad::Config::new(libp2p::kad::PROTOCOL_NAME));

        let kademlia: Toggle<Kademlia<MemoryStore>> = protocols
            .kad
            .then(|| Kademlia::with_config(peer_id, store, kad_config))
            .into();

        let autonat_v1 = protocols
            .autonat_v1
            .then(|| autonat::Behaviour::new(peer_id, Default::default()))
            .into();

        let autonat_v2_client = protocols
            .autonat_v2_client
            .then(|| {
                autonat::v2::client::Behaviour::new(
                    OsRng,
                    config.autonat_v2_client_config.unwrap_or_default(),
                )
            })
            .into();

        let autonat_v2_server = protocols
            .autonat_v2_server
            .then(|| autonat::v2::server::Behaviour::default())
            .into();

        let ping = protocols
            .ping
            .then(|| Ping::new(config.ping_config.unwrap_or_default()))
            .into();

        let identify = protocols
            .identify
            .then(|| {
                Identify::new(config.identify_config.unwrap_or_else(|| {
                    libp2p::identify::Config::new("/ipfs/id".into(), keypair.public())
                }))
            })
            .into();

        // TODO: Use `Optional<Either<bool, bool>>` in protocols instead?
        let pubsub = match (protocols.floodsub, protocols.gossipsub) {
            (true, true) => {
                return Err(std::io::Error::other(
                    "cannot support floodsub and gossipsub at the same time",
                ));
            }
            (true, false) => {
                let config = config
                    .floodsub_config
                    .unwrap_or_else(|| libp2p::floodsub::FloodsubConfig::new(peer_id));
                let behaviour = libp2p::floodsub::Floodsub::from_config(config);
                Either::Right(Some(behaviour).into())
            }
            (false, true) => {
                let config = config.gossipsub_config.unwrap_or_default();
                // TODO: Customize message authenticity
                let behaviour = libp2p::gossipsub::Behaviour::new(
                    libp2p::gossipsub::MessageAuthenticity::Signed(keypair.clone()),
                    config,
                )
                .map_err(std::io::Error::other)?;
                Either::Left(Some(behaviour).into())
            }
            (false, false) => Either::Left(None.into()),
        };

        #[cfg(not(target_arch = "wasm32"))]
        let dcutr = protocols.dcutr.then(|| Dcutr::new(peer_id)).into();

        let relay = protocols
            .relay_server
            .then(|| RelayServer::new(peer_id, config.relay_server_config))
            .into();

        #[cfg(not(target_arch = "wasm32"))]
        let upnp = protocols
            .upnp
            .then(libp2p::upnp::tokio::Behaviour::default)
            .into();

        let (transport, relay_client) = match protocols.relay_client {
            true => {
                let (transport, client) = client::new(peer_id);
                (Some(transport), Some(client).into())
            }
            false => (None, None.into()),
        };

        let block_list = libp2p_allow_block_list::Behaviour::default();
        let custom = Toggle::from(custom_behaviour);

        let rendezvous_client = protocols
            .rendezvous_client
            .then(|| libp2p::rendezvous::client::Behaviour::new(keypair.clone()))
            .into();

        let rendezvous_server = protocols
            .rendezvous_server
            .then(|| libp2p::rendezvous::server::Behaviour::new(Default::default()))
            .into();

        #[cfg(feature = "stream")]
        let stream = protocols.streams.then(libp2p_stream::Behaviour::new).into();

        let connection_limits = protocols
            .connection_limits
            .then(|| config.connection_limits.unwrap_or_default())
            .map(libp2p_connection_limits::Behaviour::new)
            .into();

        let mut behaviour = Behaviour {
            connection_limits,
            #[cfg(not(target_arch = "wasm32"))]
            mdns,
            kademlia,
            ping,
            identify,
            autonat_v1,
            autonat_v2_client,
            autonat_v2_server,
            pubsub,
            #[cfg(not(target_arch = "wasm32"))]
            dcutr,
            relay,
            relay_client,
            block_list,
            #[cfg(feature = "stream")]
            stream,
            #[cfg(not(target_arch = "wasm32"))]
            upnp,
            custom,
            rendezvous_client,
            rendezvous_server,
            rr_man: Toggle::from(None),
            rr_0: Toggle::from(None),
            rr_1: Toggle::from(None),
            rr_2: Toggle::from(None),
            rr_3: Toggle::from(None),
            rr_4: Toggle::from(None),
            rr_5: Toggle::from(None),
            rr_6: Toggle::from(None),
            rr_7: Toggle::from(None),
            rr_8: Toggle::from(None),
            rr_9: Toggle::from(None),
        };

        let mut existing_protocol: IndexMap<StreamProtocol, _> = IndexMap::new();

        for (index, config) in config.request_response_config.iter().enumerate() {
            let protocol =
                StreamProtocol::try_from_owned(config.protocol.clone()).expect("valid protocol");
            if existing_protocol.contains_key(&protocol) {
                tracing::warn!(%protocol, "request-response protocol is already registered");
                continue;
            };

            match index {
                0 => {
                    if behaviour.rr_0.is_enabled() {
                        continue;
                    }
                    behaviour.rr_0 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                1 => {
                    if behaviour.rr_1.is_enabled() {
                        continue;
                    }
                    behaviour.rr_1 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                2 => {
                    if behaviour.rr_2.is_enabled() {
                        continue;
                    }
                    behaviour.rr_2 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                3 => {
                    if behaviour.rr_3.is_enabled() {
                        continue;
                    }
                    behaviour.rr_3 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                4 => {
                    if behaviour.rr_4.is_enabled() {
                        continue;
                    }
                    behaviour.rr_4 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                5 => {
                    if behaviour.rr_5.is_enabled() {
                        continue;
                    }
                    behaviour.rr_5 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                6 => {
                    if behaviour.rr_6.is_enabled() {
                        continue;
                    }
                    behaviour.rr_6 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                7 => {
                    if behaviour.rr_7.is_enabled() {
                        continue;
                    }
                    behaviour.rr_7 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                8 => {
                    if behaviour.rr_8.is_enabled() {
                        continue;
                    }
                    behaviour.rr_8 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                9 => {
                    if behaviour.rr_9.is_enabled() {
                        continue;
                    }
                    behaviour.rr_9 = protocols
                        .request_response
                        .then(|| request_response::Behaviour::new(config.clone()))
                        .into();
                }
                _ => {
                    tracing::warn!(
                        "local node can only support up to 10 request-response protocols at this time."
                    );
                    break;
                }
            }

            existing_protocol.insert(protocol, index);
        }

        if !existing_protocol.is_empty() {
            behaviour.rr_man = Toggle::from(Some(rr_man::Behaviour::new(existing_protocol)))
        }

        Ok((behaviour, transport))
    }

    pub(crate) fn request_response(
        &mut self,
        protocol: Option<StreamProtocol>,
    ) -> Option<&mut request_response::Behaviour> {
        let Some(protocol) = protocol else {
            return self.rr_0.as_mut();
        };

        let manager = self.rr_man.as_ref()?;
        let index = manager.get_protocol(protocol)?;
        match index {
            0 => self.rr_0.as_mut(),
            1 => self.rr_1.as_mut(),
            2 => self.rr_2.as_mut(),
            3 => self.rr_3.as_mut(),
            4 => self.rr_4.as_mut(),
            5 => self.rr_5.as_mut(),
            6 => self.rr_6.as_mut(),
            7 => self.rr_7.as_mut(),
            8 => self.rr_8.as_mut(),
            9 => self.rr_9.as_mut(),
            _ => None,
        }
    }
}
