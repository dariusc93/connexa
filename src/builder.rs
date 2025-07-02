mod executor;
mod transport;

#[cfg(feature = "request-response")]
use crate::behaviour::request_response::RequestResponseConfig;
#[cfg(feature = "dns")]
use crate::builder::transport::DnsResolver;
use crate::builder::transport::{
    TTransport, TransportConfig, TryIntoTransport, build_other_transport,
};
use crate::handle::Connexa;
use crate::prelude::PeerId;
use crate::task::ConnexaTask;
use crate::{TEventCallback, TPollableCallback, TSwarmEventCallback, TTaskCallback, behaviour};
use executor::ConnexaExecutor;
#[cfg(feature = "autonat")]
use libp2p::autonat::v1::Config as AutonatV1Config;
#[cfg(feature = "autonat")]
use libp2p::autonat::v2::client::Config as AutonatV2ClientConfig;
#[cfg(feature = "floodsub")]
use libp2p::floodsub::Config as FloodsubConfig;
#[cfg(feature = "identify")]
use libp2p::identify::Config as IdentifyConfig;
use libp2p::identity::Keypair;
#[cfg(feature = "kad")]
use libp2p::kad::Config as KadConfig;
#[cfg(feature = "ping")]
use libp2p::ping::Config as PingConfig;
#[cfg(feature = "pnet")]
#[cfg(not(target_arch = "wasm32"))]
use libp2p::pnet::PreSharedKey;
#[cfg(feature = "relay")]
use libp2p::relay::Config as RelayServerConfig;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Swarm, Transport};
use libp2p_connection_limits::ConnectionLimits;
use std::fmt::Debug;
use std::task::{Context, Poll};
// Since this used for quic duration, we will feature gate it to satisfy lint
#[cfg(feature = "quic")]
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use tracing::Span;

#[derive(Debug, Copy, Clone)]
pub enum FileDescLimit {
    Max,
    Custom(u64),
}

pub struct ConnexaBuilder<X, C, T>
where
    C: NetworkBehaviour,
    C: Send,
    C::ToSwarm: Debug,
    T: Send + Sync + 'static,
    X: Default + Send + Sync + 'static,
{
    keypair: Keypair,
    context: X,
    custom_behaviour: Option<C>,
    file_descriptor_limits: Option<FileDescLimit>,
    custom_task_callback: TTaskCallback<C, X, T>,
    custom_event_callback: TEventCallback<C, X>,
    swarm_event_callback: TSwarmEventCallback<C>,
    custom_pollable_callback: TPollableCallback<C, X>,
    config: Config,
    swarm_config: Box<dyn Fn(libp2p::swarm::Config) -> libp2p::swarm::Config>,
    transport_config: TransportConfig,
    custom_transport: Option<TTransport>,
    protocols: Protocols,
}

pub(crate) struct Config {
    #[cfg(feature = "kad")]
    pub kademlia_config: (String, Box<dyn Fn(KadConfig) -> KadConfig>),
    #[cfg(feature = "gossipsub")]
    pub gossipsub_config:
        Box<dyn Fn(libp2p::gossipsub::ConfigBuilder) -> libp2p::gossipsub::ConfigBuilder>,
    #[cfg(feature = "floodsub")]
    pub floodsub_config: Box<dyn Fn(FloodsubConfig) -> FloodsubConfig>,
    #[cfg(feature = "ping")]
    pub ping_config: Box<dyn Fn(PingConfig) -> PingConfig>,
    #[cfg(feature = "autonat")]
    pub autonat_v1_config: Box<dyn Fn(AutonatV1Config) -> AutonatV1Config>,
    #[cfg(feature = "autonat")]
    pub autonat_v2_client_config: Box<dyn Fn(AutonatV2ClientConfig) -> AutonatV2ClientConfig>,
    #[cfg(feature = "relay")]
    pub relay_server_config: Box<dyn Fn(RelayServerConfig) -> RelayServerConfig>,
    #[cfg(feature = "identify")]
    pub identify_config: (String, Box<dyn Fn(IdentifyConfig) -> IdentifyConfig>),
    #[cfg(feature = "request-response")]
    pub request_response_config: Vec<RequestResponseConfig>,
    pub allow_list: Vec<PeerId>,
    pub deny_list: Vec<PeerId>,
    pub connection_limits: Box<dyn Fn(ConnectionLimits) -> ConnectionLimits>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            #[cfg(feature = "kad")]
            kademlia_config: ("/ipfs/kad/1.0.0".to_string(), Box::new(|config| config)),
            #[cfg(feature = "gossipsub")]
            gossipsub_config: Box::new(|config| config),
            #[cfg(feature = "floodsub")]
            floodsub_config: Box::new(|config| config),
            #[cfg(feature = "ping")]
            ping_config: Box::new(|config| config),
            #[cfg(feature = "autonat")]
            autonat_v1_config: Box::new(|config| config),
            #[cfg(feature = "autonat")]
            autonat_v2_client_config: Box::new(|config| config),
            #[cfg(feature = "relay")]
            relay_server_config: Box::new(|config| config),
            #[cfg(feature = "identify")]
            identify_config: (String::from("/ipfs/id"), Box::new(|config| config)),
            #[cfg(feature = "request-response")]
            request_response_config: vec![],
            allow_list: Vec::new(),
            deny_list: Vec::new(),
            connection_limits: Box::new(|config| config),
        }
    }
}

#[derive(Default)]
pub(crate) struct Protocols {
    #[cfg(feature = "gossipsub")]
    pub(crate) gossipsub: bool,
    #[cfg(feature = "floodsub")]
    pub(crate) floodsub: bool,
    #[cfg(feature = "kad")]
    pub(crate) kad: bool,
    #[cfg(feature = "relay")]
    pub(crate) relay_client: bool,
    #[cfg(feature = "relay")]
    pub(crate) relay_server: bool,
    #[cfg(feature = "dcutr")]
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) dcutr: bool,
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "mdns")]
    pub(crate) mdns: bool,
    #[cfg(feature = "identify")]
    pub(crate) identify: bool,
    #[cfg(feature = "autonat")]
    pub(crate) autonat_v1: bool,
    #[cfg(feature = "autonat")]
    pub(crate) autonat_v2_client: bool,
    #[cfg(feature = "autonat")]
    pub(crate) autonat_v2_server: bool,
    #[cfg(feature = "rendezvous")]
    pub(crate) rendezvous_client: bool,
    #[cfg(feature = "rendezvous")]
    pub(crate) rendezvous_server: bool,
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "upnp")]
    pub(crate) upnp: bool,
    #[cfg(feature = "ping")]
    pub(crate) ping: bool,
    #[cfg(feature = "stream")]
    pub(crate) streams: bool,
    #[cfg(feature = "request-response")]
    pub(crate) request_response: bool,
    pub(crate) connection_limits: bool,
    pub(crate) allow_list: bool,
    pub(crate) deny_list: bool,
}

impl<X, C, T> ConnexaBuilder<X, C, T>
where
    C: NetworkBehaviour,
    C: Send,
    C::ToSwarm: Debug,
    T: Send + Sync + 'static,
    X: Default + Unpin + Send + Sync + 'static,
{
    /// Create a new instance
    pub fn new_identity() -> Self {
        let keypair = Keypair::generate_ed25519();
        Self::with_existing_identity(&keypair)
    }

    /// Create an instance with an existing keypair.
    pub fn with_existing_identity(keypair: &Keypair) -> Self {
        let keypair = keypair.clone();
        Self {
            keypair,
            custom_behaviour: None,
            context: X::default(),
            file_descriptor_limits: None,
            custom_task_callback: Box::new(|_, _, _| ()),
            custom_event_callback: Box::new(|_, _, _| ()),
            swarm_event_callback: Box::new(|_| ()),
            custom_pollable_callback: Box::new(|_, _, _| Poll::Pending),
            config: Config::default(),
            protocols: Protocols::default(),
            swarm_config: Box::new(|config| config),
            transport_config: TransportConfig::default(),
            custom_transport: None,
        }
    }

    /// Configuration for the swarm.
    pub fn set_swarm_config<F>(mut self, f: F) -> Self
    where
        F: Fn(libp2p::swarm::Config) -> libp2p::swarm::Config + 'static,
    {
        self.swarm_config = Box::new(f);
        self
    }

    /// Set a file descriptor limit.
    /// Note that this is only available on Unix-based operating systems, while others will only output
    /// a warning in the logs
    pub fn set_file_descriptor_limit(mut self, limit: FileDescLimit) -> Self {
        self.file_descriptor_limits = Some(limit);
        self
    }

    /// Set a callback for custom task events.
    pub fn set_custom_task_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, T) + 'static + Send,
    {
        self.custom_task_callback = Box::new(f);
        self
    }

    /// Handles events from the custom behaviour.
    pub fn set_custom_event_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, C::ToSwarm) + 'static + Send,
    {
        self.custom_event_callback = Box::new(f);
        self
    }

    /// Sets a custom callback for handling polling operations
    /// Note that regardless of if the Fn returns Poll::Ready or Poll::Pending that
    /// it would be no-op since this is just to process futures or streams that may be held in context
    pub fn set_pollable_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut Context<'_>, &mut Swarm<behaviour::Behaviour<C>>, &mut X) -> Poll<()>
            + Send
            + 'static,
    {
        self.custom_pollable_callback = Box::new(f);
        self
    }

    /// Handles libp2p swarm events
    pub fn set_swarm_event_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&SwarmEvent<behaviour::BehaviourEvent<C>>) + 'static + Send,
    {
        self.swarm_event_callback = Box::new(f);
        self
    }

    pub fn set_context(mut self, context: X) -> Self {
        self.context = context;
        self
    }

    /// Enables kademlia
    #[cfg(feature = "kad")]
    pub fn with_kademlia(self) -> Self {
        self.with_kademlia_with_config("/ipfs/kad/1.0.0", |config| config)
    }

    /// Enables kademlia with custom configuration
    #[cfg(feature = "kad")]
    pub fn with_kademlia_with_config<F>(mut self, protocol: impl Into<String>, f: F) -> Self
    where
        F: Fn(KadConfig) -> KadConfig + 'static,
    {
        self.protocols.kad = true;
        self.config.kademlia_config = (protocol.into(), Box::new(f));
        self
    }

    /// Enable mdns
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "mdns")]
    pub fn with_mdns(mut self) -> Self {
        self.protocols.mdns = true;
        self
    }

    /// Enable relay client
    #[cfg(feature = "relay")]
    pub fn with_relay(mut self) -> Self {
        self.protocols.relay_client = true;
        self
    }

    /// Enables DCuTR
    #[cfg(all(feature = "relay", feature = "dcutr"))]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_dcutr(mut self) -> Self {
        self.protocols.dcutr = true;
        self
    }

    /// Enable relay server
    #[cfg(feature = "relay")]
    pub fn with_relay_server(self) -> Self {
        self.with_relay_server_with_config(|config| config)
    }

    /// Enable relay server with custom configuration
    #[cfg(feature = "relay")]
    pub fn with_relay_server_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(RelayServerConfig) -> RelayServerConfig + 'static,
    {
        self.protocols.relay_server = true;
        self.config.relay_server_config = Box::new(config);
        self
    }

    /// Enable port mapping (AKA UPnP)
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "upnp")]
    pub fn with_upnp(mut self) -> Self {
        self.protocols.upnp = true;
        self
    }

    /// Enables rendezvous server
    #[cfg(feature = "rendezvous")]
    pub fn with_rendezvous_server(mut self) -> Self {
        self.protocols.rendezvous_server = true;
        self
    }

    /// Enables rendezvous client
    #[cfg(feature = "rendezvous")]
    pub fn with_rendezvous_client(mut self) -> Self {
        self.protocols.rendezvous_client = true;
        self
    }

    /// Enables identify
    #[cfg(feature = "identify")]
    pub fn with_identify(self) -> Self {
        self.with_identify_with_config("/ipfs/id", |config| config)
    }

    /// Enables identify with custom configuration
    #[cfg(feature = "identify")]
    pub fn with_identify_with_config<F>(mut self, protocol: impl Into<String>, config: F) -> Self
    where
        F: Fn(IdentifyConfig) -> IdentifyConfig + 'static,
    {
        let protocol = protocol.into();
        self.protocols.identify = true;
        self.config.identify_config = (protocol, Box::new(config));
        self
    }

    /// Enables stream
    #[cfg(feature = "stream")]
    pub fn with_streams(mut self) -> Self {
        self.protocols.streams = true;
        self
    }

    /// Enables gossipsub
    #[cfg(feature = "gossipsub")]
    pub fn with_gossipsub(self) -> Self {
        self.with_gossipsub_with_config(|config| config)
    }

    /// Enables gossipsub with custom configuration
    #[cfg(feature = "gossipsub")]
    pub fn with_gossipsub_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(libp2p::gossipsub::ConfigBuilder) -> libp2p::gossipsub::ConfigBuilder + 'static,
    {
        self.protocols.gossipsub = true;
        self.config.gossipsub_config = Box::new(config);
        self
    }

    /// Enables floodsub
    #[cfg(feature = "floodsub")]
    pub fn with_floodsub(self) -> Self {
        self.with_floodsub_with_config(|config| config)
    }

    /// Enables floodsub with custom configuration
    #[cfg(feature = "floodsub")]
    pub fn with_floodsub_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(FloodsubConfig) -> FloodsubConfig + 'static,
    {
        self.protocols.floodsub = true;
        self.config.floodsub_config = Box::new(config);
        self
    }

    /// Enables request response.
    /// Note: At this time, this option will only support up to 10 request-response behaviours.
    ///       with any additional being ignored. Additionally, any duplicated protocols that are
    ///       provided will be ignored.
    #[cfg(feature = "request-response")]
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

    /// Enables autonat v1
    #[cfg(feature = "autonat")]
    pub fn with_autonat_v1(self) -> Self {
        self.with_autonat_v1_with_config(|config| config)
    }

    /// Enables autonat v1 with custom configuration
    #[cfg(feature = "autonat")]
    pub fn with_autonat_v1_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(AutonatV1Config) -> AutonatV1Config + 'static,
    {
        self.protocols.autonat_v1 = true;
        self.config.autonat_v1_config = Box::new(config);
        self
    }

    /// Enables autonat v2 client
    #[cfg(feature = "autonat")]
    pub fn with_autonat_v2_client(self) -> Self {
        self.with_autonat_v2_client_with_config(|config| config)
    }

    /// Enables autonat v2 client with custom configuration
    #[cfg(feature = "autonat")]
    pub fn with_autonat_v2_client_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(AutonatV2ClientConfig) -> AutonatV2ClientConfig + 'static,
    {
        self.protocols.autonat_v2_client = true;
        self.config.autonat_v2_client_config = Box::new(config);
        self
    }

    /// Enables autonat v2 server
    #[cfg(feature = "autonat")]
    pub fn with_autonat_v2_server(mut self) -> Self {
        self.protocols.autonat_v2_server = true;
        self
    }

    /// Enables ping
    #[cfg(feature = "ping")]
    pub fn with_ping(self) -> Self {
        self.with_ping_with_config(|config| config)
    }

    /// Enables ping with custom configuration
    #[cfg(feature = "ping")]
    pub fn with_ping_with_config<F>(mut self, config: F) -> Self
    where
        F: Fn(PingConfig) -> PingConfig + 'static,
    {
        self.protocols.ping = true;
        self.config.ping_config = Box::new(config);
        self
    }

    pub fn with_whitelist(self) -> Self {
        self.with_whitelist_with_list([])
    }

    pub fn with_whitelist_with_list(mut self, list: impl IntoIterator<Item = PeerId>) -> Self {
        self.config.allow_list = list.into_iter().collect();
        self.protocols.allow_list = true;
        self
    }

    pub fn with_blacklist(self) -> Self {
        self.with_blacklist_with_list([])
    }

    pub fn with_blacklist_with_list(mut self, list: impl IntoIterator<Item = PeerId>) -> Self {
        self.protocols.deny_list = true;
        self.config.deny_list = list.into_iter().collect();
        self
    }

    /// Enables connection limits.
    pub fn with_connection_limits(self) -> Self {
        self.with_connection_limits_with_config(|config| config)
    }

    /// Enables connection limits with custom configuration.
    pub fn with_connection_limits_with_config<F>(mut self, f: F) -> Self
    where
        F: Fn(ConnectionLimits) -> ConnectionLimits + 'static,
    {
        self.protocols.connection_limits = true;
        self.config.connection_limits = Box::new(f);
        self
    }

    /// Set a custom behaviour
    /// Note that if you want to communicate or interact with the behaviour that you would need to set a callback via
    /// `custom_event_callback` and `custom_task_callback`.
    pub fn with_custom_behaviour<F>(mut self, f: F) -> Self
    where
        F: Fn(&Keypair) -> C,
        F: 'static,
    {
        let behaviour = f(&self.keypair);
        self.custom_behaviour = Some(behaviour);
        self
    }

    /// Set a custom behaviour with context
    /// Note that if you want to communicate or interact with the behaviour that you would need to set a callback via
    /// `custom_event_callback` and `custom_task_callback`.
    pub fn with_custom_behaviour_with_context<F, IC>(mut self, context: IC, f: F) -> Self
    where
        F: Fn(&Keypair, IC) -> C,
        F: 'static,
    {
        let behaviour = f(&self.keypair, context);
        self.custom_behaviour = Some(behaviour);
        self
    }

    /// Enables quic transport
    #[cfg(feature = "quic")]
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Enables quic transport with custom configuration
    #[cfg(feature = "quic")]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_quic_with_config<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut libp2p::quic::Config) + 'static,
    {
        let callback = Box::new(f);
        self.transport_config.quic_config_callback = callback;
        self.transport_config.enable_quic = true;
        self
    }

    /// Enables tcp transport
    #[cfg(feature = "tcp")]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_tcp(self) -> Self {
        self.enable_tcp_with_config(|config| config.nodelay(true))
    }

    /// Enables tcp transport with custom configuration
    #[cfg(feature = "tcp")]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_tcp_with_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(libp2p::tcp::Config) -> libp2p::tcp::Config + 'static,
    {
        let callback = Box::new(f);
        self.transport_config.tcp_config_callback = callback;
        self.transport_config.enable_tcp = true;
        self
    }

    /// Enables pnet transport
    #[cfg(feature = "pnet")]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_pnet(mut self, psk: PreSharedKey) -> Self {
        self.transport_config.enable_pnet = true;
        self.transport_config.pnet_psk = Some(psk);
        self
    }

    /// Enables websocket transport
    #[cfg(feature = "websocket")]
    pub fn enable_websocket(mut self) -> Self {
        self.transport_config.enable_websocket = true;
        self
    }

    /// Enables secure websocket transport
    #[cfg(feature = "websocket")]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_secure_websocket(mut self, pem: Option<(Vec<String>, String)>) -> Self {
        self.transport_config.enable_secure_websocket = true;
        self.transport_config.enable_websocket = true;
        self.transport_config.websocket_pem = pem;
        self
    }

    /// Enables DNS
    #[cfg(feature = "dns")]
    pub fn enable_dns(self) -> Self {
        self.enable_dns_with_resolver(DnsResolver::default())
    }

    /// Enables DNS with a specific resolver
    #[cfg(feature = "dns")]
    pub fn enable_dns_with_resolver(mut self, resolver: DnsResolver) -> Self {
        self.transport_config.dns_resolver = Some(resolver);
        self.transport_config.enable_dns = true;
        self
    }

    /// Enables memory transport
    pub fn enable_memory_transport(mut self) -> Self {
        self.transport_config.enable_memory_transport = true;
        self
    }

    /// Implements custom transport that will override the existing transport construction.
    pub fn with_custom_transport<F, M, TP, R>(mut self, f: F) -> std::io::Result<Self>
    where
        M: libp2p::core::muxing::StreamMuxer + Send + 'static,
        M::Substream: Send,
        M::Error: Send + Sync,
        TP: Transport<Output = (PeerId, M)> + Send + Unpin + 'static,
        TP::Error: Send + Sync + 'static,
        TP::Dial: Send,
        TP::ListenerUpgrade: Send,
        R: TryIntoTransport<TP>,
        F: FnOnce(&Keypair) -> R,
    {
        let transport = build_other_transport(&self.keypair, f)?;
        self.custom_transport = Some(transport);
        Ok(self)
    }

    pub fn build(self) -> std::io::Result<Connexa<T>> {
        let ConnexaBuilder {
            keypair,
            context,
            custom_behaviour,
            file_descriptor_limits,
            custom_task_callback,
            custom_event_callback,
            swarm_event_callback,
            custom_pollable_callback,
            config,
            protocols,
            swarm_config,
            transport_config,
            custom_transport,
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

        let transport = match custom_transport {
            Some(custom_transport) => custom_transport.boxed(),
            None => transport::build_transport(&keypair, relay_transport, transport_config)?,
        };

        let swarm = Swarm::new(transport, behaviour, peer_id, swarm_config);

        let connexa_task = ConnexaTask::new(swarm);

        let to_task = async_rt::task::spawn_coroutine_with_context(
            (
                context,
                custom_task_callback,
                custom_event_callback,
                swarm_event_callback,
                custom_pollable_callback,
                connexa_task,
            ),
            |(context, tcb, ecb, scb, pcb, mut ctx), rx| async move {
                ctx.set_context(context);
                ctx.set_task_callback(tcb);
                ctx.set_event_callback(ecb);
                ctx.set_command_receiver(rx);
                ctx.set_swarm_event_callback(scb);
                ctx.set_pollable_callback(pcb);

                ctx.await
            },
        );

        let connexa = Connexa::new(span, keypair, to_task);

        Ok(connexa)
    }
}
