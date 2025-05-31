mod dht;
mod floodsub;
mod gossipsub;
mod rendezvous;
mod request_response;
#[cfg(feature = "stream")]
mod stream;
mod swarm;

use crate::handle::dht::ConnexaDht;
use crate::handle::floodsub::ConnexaFloodsub;
use crate::handle::gossipsub::ConnexaGossipsub;
use crate::handle::rendezvous::ConnexaRendezvous;
use crate::handle::request_response::ConnexaRequestResponse;
#[cfg(feature = "stream")]
use crate::handle::stream::ConnexaStream;
use crate::handle::swarm::ConnexaSwarm;
use crate::types::Command;
use async_rt::CommunicationTask;
use libp2p::identity::Keypair;
use std::fmt::Debug;
use tracing::Span;

type Result<T> = std::io::Result<T>;

#[derive(Clone)]
pub struct Connexa<T = ()> {
    #[allow(dead_code)]
    span: Span,
    keypair: Keypair,
    to_task: CommunicationTask<Command<T>>,
}

impl<T> Connexa<T> {
    pub(crate) fn new(
        span: Span,
        keypair: Keypair,
        to_task: CommunicationTask<Command<T>>,
    ) -> Self {
        Self {
            span,
            keypair,
            to_task,
        }
    }
}

impl Debug for Connexa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connexa")
            .field("public_key", &self.keypair.public())
            .finish()
    }
}

impl Connexa {
    pub fn swarm(&self) -> ConnexaSwarm {
        ConnexaSwarm::new(self)
    }

    pub fn floodsub(&self) -> ConnexaFloodsub {
        ConnexaFloodsub::new(self)
    }

    pub fn gossipsub(&self) -> ConnexaGossipsub {
        ConnexaGossipsub::new(self)
    }

    pub fn dht(&self) -> ConnexaDht {
        ConnexaDht::new(self)
    }

    pub fn request_response(&self) -> ConnexaRequestResponse {
        ConnexaRequestResponse::new(self)
    }

    #[cfg(feature = "stream")]
    pub fn stream(&self) -> ConnexaStream {
        ConnexaStream::new(self)
    }

    pub fn rendezvous(&self) -> ConnexaRendezvous {
        ConnexaRendezvous::new(self)
    }
}

impl<T> Connexa<T> {
    pub async fn send_custom_event(&self, event: T) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        self.to_task.clone().send(Command::Custom(event)).await
    }
}
