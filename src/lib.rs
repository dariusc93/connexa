pub mod behaviour;
pub mod builder;
pub mod error;
pub mod handle;
pub mod task;
pub(crate) mod types;

use crate::behaviour::BehaviourEvent;
use crate::prelude::{Swarm, SwarmEvent};
use libp2p::swarm::NetworkBehaviour;
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
    pub use libp2p::{
        Multiaddr, PeerId, StreamProtocol, identity::*, multiaddr::Protocol, swarm::*,
    };

    #[cfg(feature = "kad")]
    pub mod dht {
        pub use libp2p::kad::*;
    }

    pub mod transport {
        pub use libp2p::core::muxing;
        pub use libp2p::core::transport;
        pub use libp2p::core::upgrade;
        pub use libp2p::noise;
        pub use libp2p::yamux;
    }

    pub type DefaultConnexaBuilder = ConnexaBuilder<(), super::dummy::Behaviour, ()>;
}
