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

impl<T> Debug for Connexa<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connexa")
            .field("public_key", &self.keypair.public())
            .finish()
    }
}

impl<T> Connexa<T>
where
    T: Send + Sync + 'static,
{
    /// Returns a handle for swarm functions
    pub fn swarm(&self) -> ConnexaSwarm<T> {
        ConnexaSwarm::new(self)
    }

    /// Returns a handle for floodsub functions
    pub fn floodsub(&self) -> ConnexaFloodsub<T> {
        ConnexaFloodsub::new(self)
    }

    /// Returns a handle for gossipsub functions   
    pub fn gossipsub(&self) -> ConnexaGossipsub<T> {
        ConnexaGossipsub::new(self)
    }

    /// Returns a handle for dht functions  
    pub fn dht(&self) -> ConnexaDht<T> {
        ConnexaDht::new(self)
    }

    /// Returns a handle for request-response functions
    pub fn request_response(&self) -> ConnexaRequestResponse<T> {
        ConnexaRequestResponse::new(self)
    }

    /// Returns a handle for stream functions
    #[cfg(feature = "stream")]
    pub fn stream(&self) -> ConnexaStream<T> {
        ConnexaStream::new(self)
    }

    /// Returns a handle for rendezvous functions
    pub fn rendezvous(&self) -> ConnexaRendezvous<T> {
        ConnexaRendezvous::new(self)
    }

    /// Shuts down the underlining task
    /// Note that this does not gracefully shut down the task
    pub fn shutdown(self) {
        self.to_task.abort();
    }
}

impl<T> Connexa<T> {
    /// Send a custom event to the running task that can be handled by the set `ConnexaTask::custom_task_callback`
    pub async fn send_custom_event(&self, event: T) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        self.to_task.clone().send(Command::Custom(event)).await
    }
}
