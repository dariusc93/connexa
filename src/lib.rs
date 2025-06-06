pub mod behaviour;
pub mod builder;
pub mod error;
pub mod handle;
pub mod task;
pub(crate) mod types;

pub mod dummy {
    pub use crate::behaviour::dummy::{Behaviour, DummyHandler};
}

pub mod prelude {
    use crate::builder::ConnexaBuilder;
    pub use crate::types::*;
    pub use libp2p::{Multiaddr, PeerId, StreamProtocol, multiaddr::Protocol, swarm::*};

    pub mod dht {
        pub use libp2p::kad::{Mode, Quorum};
    }

    pub type DefaultConnexaBuilder = ConnexaBuilder<(), super::dummy::Behaviour, ()>;
}
