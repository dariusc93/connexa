pub mod behaviour;
pub mod builder;
pub mod error;
pub mod handle;
pub mod task;
pub(crate) mod types;

use crate::behaviour::BehaviourEvent;
use libp2p::swarm::NetworkBehaviour;
use libp2p::swarm::{Swarm, SwarmEvent};
use std::task::{Context, Poll};

pub(crate) type TTaskCallback<C, X, T> =
    Box<dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, T) + 'static + Send>;
pub(crate) type TEventCallback<C, X> = Box<
    dyn Fn(&mut Swarm<behaviour::Behaviour<C>>, &mut X, <C as NetworkBehaviour>::ToSwarm)
        + 'static
        + Send,
>;
pub(crate) type TPollableCallback<C, X> = Box<
    dyn Fn(&mut Context, &mut Swarm<behaviour::Behaviour<C>>, &mut X) -> Poll<()> + 'static + Send,
>;
pub(crate) type TSwarmEventCallback<C> =
    Box<dyn Fn(&SwarmEvent<BehaviourEvent<C>>) + 'static + Send>;

pub mod dummy {
    pub use crate::behaviour::dummy::{Behaviour, DummyHandler};
}

pub mod prelude {
    use crate::builder::ConnexaBuilder;
    pub use crate::types::*;
    pub use libp2p::{Multiaddr, PeerId, Stream, StreamProtocol, multiaddr::Protocol};

    pub use libp2p::identity;

    pub mod swarm {
        pub use libp2p::SwarmBuilder;
        pub use libp2p::swarm::*;
    }

    #[cfg(feature = "kad")]
    pub mod dht {
        pub use crate::handle::dht::{ToOptionalRecordKey, ToRecordKey};
        pub use libp2p::kad::*;
    }

    #[cfg(feature = "request-response")]
    pub mod request_response {
        pub use crate::handle::request_response::{IntoRequest, OptionalStreamProtocol};
        pub use libp2p::request_response::{
            Config, Event, InboundFailure, InboundRequestId, Message, OutboundFailure,
            OutboundRequestId, ProtocolSupport,
        };
    }

    #[cfg(feature = "stream")]
    pub mod stream {
        pub use crate::handle::stream::IntoStreamProtocol;
        pub use libp2p_stream::{Control, IncomingStreams, OpenStreamError};
    }

    #[cfg(feature = "relay")]
    pub mod relay {
        pub mod server {
            // TODO: Determine if CircuitId is needed
            pub use libp2p::relay::{Config, Event, RateLimiter, StatusCode};
        }

        pub mod client {
            pub use libp2p::relay::client::Event;
        }
    }

    #[cfg(feature = "dcutr")]
    pub mod dcutr {
        pub use libp2p::dcutr::{Error, Event};
    }

    #[cfg(feature = "ping")]
    pub mod ping {
        pub use libp2p::ping::{Config, Event, Failure};
    }

    #[cfg(feature = "identify")]
    pub mod identify {
        pub use libp2p::identify::{Config, Event, Info, UpgradeError};
    }

    #[cfg(feature = "gossipsub")]
    pub mod gossipsub {
        pub use crate::handle::gossipsub::IntoTopic as IntoGossipsubTopic;
        pub use libp2p::gossipsub::{
            AllowAllSubscriptionFilter, Config, ConfigBuilder, Event, IdentTopic, Message,
            MessageAcceptance, MessageAuthenticity, MessageId, Sha256Topic, Topic, TopicHash,
            ValidationMode, Version,
        };
    }

    #[cfg(feature = "floodsub")]
    pub mod floodsub {
        pub use crate::handle::floodsub::IntoTopic as IntoFloodsubTopic;
        pub use libp2p::floodsub::{Config, Event, Topic};
    }

    #[cfg(feature = "rendezvous")]
    pub mod rendezvous {
        pub use libp2p::rendezvous::{
            Cookie, ErrorCode, MAX_NAMESPACE, MAX_TTL, MIN_TTL, Namespace, Registration,
        };
    }

    #[cfg(feature = "mdns")]
    pub mod mdns {
        pub use libp2p::mdns::{Config, Event};
    }

    #[cfg(feature = "autonat")]
    pub mod autonat {
        pub mod v1 {
            pub use libp2p::autonat::v1::{
                Config, Event, InboundFailure, InboundProbeError, InboundProbeEvent,
            };
        }

        pub mod v2 {
            pub mod server {
                pub use libp2p::autonat::v2::server::Event;
            }

            pub mod client {
                pub use libp2p::autonat::v2::client::{Config, Event};
            }
        }
    }

    #[cfg(feature = "upnp")]
    pub mod upnp {
        pub use libp2p::upnp::Event;
    }

    pub mod connection_limits {
        pub use libp2p_connection_limits::{ConnectionLimits, Exceeded};
    }

    pub mod transport {
        pub use libp2p::core::muxing;
        pub use libp2p::core::transport;
        pub use libp2p::core::upgrade;
        pub use libp2p::core::{ConnectedPoint, Endpoint};
        #[cfg(feature = "noise")]
        pub use libp2p::noise;
        #[cfg(feature = "tcp")]
        pub use libp2p::tcp;
        #[cfg(feature = "websocket")]
        pub use libp2p::websocket;
        #[cfg(feature = "yamux")]
        pub use libp2p::yamux;
    }

    pub type DefaultConnexaBuilder = ConnexaBuilder<(), super::dummy::Behaviour, ()>;
}
