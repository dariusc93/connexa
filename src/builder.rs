mod executor;
mod transport;

use crate::behaviour;
use crate::behaviour::request_response::RequestResponseConfig;
#[cfg(feature = "dns")]
use crate::builder::transport::DnsResolver;
use crate::builder::transport::TransportConfig;
use crate::handle::Connexa;
use crate::task::ConnexaTask;
use executor::ConnexaExecutor;
use libp2p::Swarm;
use libp2p::autonat::v1::Config as AutonatV1Config;
use libp2p::autonat::v2::client::Config as AutonatV2ClientConfig;
use libp2p::floodsub::FloodsubConfig;
use libp2p::gossipsub::Config as GossipsubConfig;
use libp2p::identify::Config as IdentifyConfig;
use libp2p::identity::Keypair;
use libp2p::kad::Config as KadConfig;
use libp2p::ping::Config as PingConfig;
#[cfg(feature = "pnet")]
use libp2p::pnet::PreSharedKey;
use libp2p::relay::Config as RelayServerConfig;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p_connection_limits::ConnectionLimits;
use std::fmt::Debug;
// Since this used for quic duration, we will feature gate it to satisfy lint
#[cfg(feature = "quic")]
use std::time::Duration;
use tracing::Span;

#[derive(Debug, Copy, Clone)]
pub enum FileDescLimit {
    Max,
    Custom(u64),
}

pub struct ConnexaBuilder<C = libp2p::swarm::dummy::Behaviour, T = ()>
where
    C: NetworkBehaviour,
    C: Send,
    C::ToSwarm: Debug,
{
    keypair: Keypair,
    custom_behaviour: Option<C>,
    file_descriptor_limits: Option<FileDescLimit>,
    custom_task_callback: Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, T) + 'static + Send>,
    custom_event_callback:
        Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, C::ToSwarm) + 'static + Send>,
    swarm_event_callback: Box<dyn Fn(&SwarmEvent<C>) + 'static + Send>,
    config: Config,
    swarm_config: Box<dyn Fn(libp2p::swarm::Config) -> libp2p::swarm::Config>,
    transport_config: TransportConfig,
    protocols: Protocols,
}

// TODO: Instead of providing the optional configuration,
//       we should instead use a FnOnce to pass the default configuration
//       and allow changes there instead, which would be used when constructing
//       the behaviour
#[derive(Default)]
pub(crate) struct Config {
    pub kademlia_config: Option<KadConfig>,
    pub gossipsub_config: Option<GossipsubConfig>,
    pub floodsub_config: Option<FloodsubConfig>,
    pub ping_config: Option<PingConfig>,
    pub autonat_v1_config: Option<AutonatV1Config>,
    pub autonat_v2_client_config: Option<AutonatV2ClientConfig>,
    pub relay_server_config: RelayServerConfig,
    pub identify_config: Option<IdentifyConfig>,
    pub request_response_config: Vec<RequestResponseConfig>,
    pub connection_limits: Option<ConnectionLimits>,
}

#[derive(Default)]
pub(crate) struct Protocols {
    pub(crate) gossipsub: bool,
    pub(crate) floodsub: bool,
    pub(crate) kad: bool,
    pub(crate) relay_client: bool,
    pub(crate) relay_server: bool,
    pub(crate) dcutr: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) mdns: bool,
    pub(crate) identify: bool,
    pub(crate) autonat_v1: bool,
    pub(crate) autonat_v2_client: bool,
    pub(crate) autonat_v2_server: bool,
    pub(crate) rendezvous_client: bool,
    pub(crate) rendezvous_server: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) upnp: bool,
    pub(crate) ping: bool,
    #[cfg(feature = "stream")]
    pub(crate) streams: bool,
    pub(crate) request_response: bool,
    pub(crate) connection_limits: bool,
}

impl<C, T> ConnexaBuilder<C, T>
where
    C: NetworkBehaviour,
    C: Send,
    C::ToSwarm: Debug,
    T: std::marker::Send + 'static,
{
    pub fn new_identity() -> Self {
        let keypair = Keypair::generate_ed25519();
        Self::with_existing_identity(&keypair)
    }

    pub fn with_existing_identity(keypair: &Keypair) -> Self {
        let keypair = keypair.clone();
        Self {
            keypair,
            custom_behaviour: None,
            file_descriptor_limits: None,
            custom_task_callback: Box::new(|_, _| ()),
            custom_event_callback: Box::new(|_, _| ()),
            swarm_event_callback: Box::new(|_| ()),
            config: Config::default(),
            protocols: Protocols::default(),
            swarm_config: Box::new(|config| config),
            transport_config: TransportConfig::default(),
        }
    }

    /// Set timeout for idle connections
    pub fn set_swarm_config<F>(mut self, f: F) -> Self
    where
        F: Fn(libp2p::swarm::Config) -> libp2p::swarm::Config + 'static,
    {
        self.swarm_config = Box::new(f);
        self
    }

    /// Enables kademlia
    pub fn with_kademlia(mut self, config: KadConfig) -> Self {
        self.protocols.kad = true;
        self.config.kademlia_config = Some(config);
        self
    }

    /// Enable mdns
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_mdns(mut self) -> Self {
        self.protocols.mdns = true;
        self
    }

    /// Enable relay client
    pub fn with_relay(mut self) -> Self {
        self.protocols.relay_client = true;
        self
    }

    pub fn with_dcutr(mut self) -> Self {
        self.protocols.dcutr = true;
        self
    }

    /// Enable relay server
    pub fn with_relay_server(mut self, config: RelayServerConfig) -> Self {
        self.protocols.relay_server = true;
        self.config.relay_server_config = config;
        self
    }

    /// Enable port mapping (AKA UPnP)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_upnp(mut self) -> Self {
        self.protocols.upnp = true;
        self
    }

    /// Enables rendezvous server
    pub fn with_rendezvous_server(mut self) -> Self {
        self.protocols.rendezvous_server = true;
        self
    }

    /// Enables rendezvous client
    pub fn with_rendezvous_client(mut self) -> Self {
        self.protocols.rendezvous_client = true;
        self
    }

    /// Enables identify
    pub fn with_identify(mut self, config: IdentifyConfig) -> Self {
        self.protocols.identify = true;
        self.config.identify_config.replace(config);
        self
    }

    #[cfg(feature = "stream")]
    pub fn with_streams(mut self) -> Self {
        self.protocols.streams = true;
        self
    }

    /// Enables pubsub
    pub fn with_gossipsub(mut self, config: GossipsubConfig) -> Self {
        self.protocols.gossipsub = true;
        self.config.gossipsub_config.replace(config);
        self
    }

    pub fn with_floodsub(mut self, config: FloodsubConfig) -> Self {
        self.protocols.floodsub = true;
        self.config.floodsub_config.replace(config);
        self
    }

    /// Enables request response.
    /// Note: At this time, this option will only support up to 10 request-response behaviours.
    ///       with any additional being ignored. Additionally, any duplicated protocols that are
    ///       provided will be ignored.
    pub fn with_request_response(mut self, mut config: Vec<RequestResponseConfig>) -> Self {
        if config.len() > 10 {
            config.truncate(10);
        }
        self.protocols.request_response = true;
        if config.is_empty() {
            config.push(RequestResponseConfig::default());
        }

        self.config.request_response_config = config;

        self
    }

    /// Enables autonat
    pub fn with_autonat_v1(mut self, config: AutonatV1Config) -> Self {
        self.protocols.autonat_v1 = true;
        self.config.autonat_v1_config.replace(config);
        self
    }

    pub fn with_autonat_v2_client(mut self, config: AutonatV2ClientConfig) -> Self {
        self.protocols.autonat_v2_client = true;
        self.config.autonat_v2_client_config.replace(config);
        self
    }

    pub fn with_autonat_v2_server(mut self) -> Self {
        self.protocols.autonat_v2_server = true;
        self
    }

    /// Enables ping
    pub fn with_ping(mut self, config: PingConfig) -> Self {
        self.protocols.ping = true;
        self.config.ping_config.replace(config);
        self
    }

    /// Set a custom behaviour
    pub fn with_custom_behaviour(mut self, behaviour: C) -> Self {
        self.custom_behaviour = Some(behaviour);
        self
    }

    #[cfg(feature = "quic")]
    pub fn enable_quic(self) -> Self {
        //Note: It might be wise to set the timeout and keepalive low on
        //      quic transport since its not properly resetting connection state when reconnecting before connection timeout
        //      While in smaller settings this would be alright, we should be cautious of this setting for nodes with larger connections
        //      since this may increase cpu and network usage.
        //      see https://github.com/libp2p/rust-libp2p/issues/5097
        self.enable_quic_with_config(|config| {
            config.keep_alive_interval = Duration::from_millis(100);
            config.max_idle_timeout = 300;
        })
    }

    #[cfg(feature = "quic")]
    pub fn enable_quic_with_config<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut libp2p::quic::Config) + 'static,
    {
        let callback = Box::new(f);
        self.transport_config.quic_config_callback = callback;
        self.transport_config.enable_quic = true;
        self
    }

    #[cfg(feature = "tcp")]
    pub fn enable_tcp(self) -> Self {
        self.enable_tcp_with_config(|config| config.nodelay(true))
    }

    #[cfg(feature = "tcp")]
    pub fn enable_tcp_with_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(libp2p::tcp::Config) -> libp2p::tcp::Config + 'static,
    {
        let callback = Box::new(f);
        self.transport_config.tcp_config_callback = callback;
        self.transport_config.enable_tcp = true;
        self
    }

    #[cfg(feature = "pnet")]
    pub fn enable_pnet(mut self, psk: PreSharedKey) -> Self {
        self.transport_config.enable_pnet = true;
        self.transport_config.pnet_psk = Some(psk);
        self
    }

    #[cfg(feature = "websocket")]
    pub fn enable_websocket(mut self) -> Self {
        self.transport_config.enable_websocket = true;
        self
    }

    #[cfg(feature = "websocket")]
    pub fn enable_secure_websocket(mut self, pem: Option<(Vec<String>, String)>) -> Self {
        self.transport_config.enable_secure_websocket = true;
        self.transport_config.enable_websocket = true;
        self.transport_config.websocket_pem = pem;
        self
    }

    #[cfg(feature = "dns")]
    pub fn enable_dns(self) -> Self {
        self.enable_dns_with_resolver(DnsResolver::default())
    }

    #[cfg(feature = "dns")]
    pub fn enable_dns_with_resolver(mut self, resolver: DnsResolver) -> Self {
        self.transport_config.dns_resolver = Some(resolver);
        self.transport_config.enable_dns = true;
        self
    }

    pub fn enable_memory_transport(mut self) -> Self {
        self.transport_config.enable_memory_transport = true;
        self
    }

    pub fn start(self) -> std::io::Result<Connexa<T>> {
        let ConnexaBuilder {
            keypair,
            custom_behaviour,
            file_descriptor_limits,
            custom_task_callback,
            custom_event_callback,
            swarm_event_callback,
            config,
            protocols,
            swarm_config,
            transport_config,
        } = self;

        let span = Span::current();

        if let Some(limit) = file_descriptor_limits {
            #[cfg(unix)]
            {
                let (_, hard) = rlimit::Resource::NOFILE.get()?;
                let limit = match limit {
                    FileDescLimit::Max => hard,
                    FileDescLimit::Custom(limit) => limit,
                };

                let target = std::cmp::min(hard, limit);
                rlimit::Resource::NOFILE.set(target, hard)?;
                let (soft, _) = rlimit::Resource::NOFILE.get()?;
                if soft < 2048 {
                    tracing::warn!("Limit is too low: {soft}");
                }
            }
            #[cfg(not(unix))]
            {
                tracing::warn!(
                    ?limit,
                    "fd limit can only be set on unix systems. Ignoring..."
                )
            }
        }

        let peer_id = keypair.public().to_peer_id();

        let swarm_config = swarm_config(libp2p::swarm::Config::with_executor(ConnexaExecutor));

        let (behaviour, relay_transport) =
            behaviour::Behaviour::new(&keypair, custom_behaviour, config, protocols)?;

        let transport =
            transport::build_transport(keypair.clone(), relay_transport, transport_config)?;

        let swarm = Swarm::new(transport, behaviour, peer_id, swarm_config);

        let connexa_task = ConnexaTask::new(swarm);

        let to_task = async_rt::task::spawn_coroutine_with_context(
            (
                custom_task_callback,
                custom_event_callback,
                swarm_event_callback,
                connexa_task,
            ),
            |(tcb, ecb, scb, mut ctx), rx| async move {
                ctx.set_task_callback(tcb);
                ctx.set_event_callback(ecb);
                ctx.set_command_receiver(rx);
                ctx.set_swarm_event_callback(scb);

                ctx.await
            },
        );

        let connexa = Connexa::new(span, keypair, to_task);

        Ok(connexa)
    }
}
