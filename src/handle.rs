#[cfg(feature = "autonat")]
mod autonat;
mod blacklist;
#[cfg(feature = "kad")]
pub(crate) mod dht;
#[cfg(feature = "floodsub")]
pub(crate) mod floodsub;
#[cfg(feature = "gossipsub")]
pub(crate) mod gossipsub;
mod peer_store;
#[cfg(feature = "rendezvous")]
pub(crate) mod rendezvous;
#[cfg(feature = "request-response")]
pub(crate) mod request_response;
#[cfg(feature = "stream")]
pub(crate) mod stream;
pub(crate) mod swarm;
mod whitelist;

#[cfg(feature = "autonat")]
use crate::handle::autonat::ConnexaAutonat;
use crate::handle::blacklist::ConnexaBlacklist;
#[cfg(feature = "kad")]
use crate::handle::dht::ConnexaDht;
#[cfg(feature = "floodsub")]
use crate::handle::floodsub::ConnexaFloodsub;
#[cfg(feature = "gossipsub")]
use crate::handle::gossipsub::ConnexaGossipsub;
use crate::handle::peer_store::ConnexaPeerstore;
#[cfg(feature = "rendezvous")]
use crate::handle::rendezvous::ConnexaRendezvous;
#[cfg(feature = "request-response")]
use crate::handle::request_response::ConnexaRequestResponse;
#[cfg(feature = "stream")]
use crate::handle::stream::ConnexaStream;
use crate::handle::swarm::ConnexaSwarm;
use crate::handle::whitelist::ConnexaWhitelist;
use crate::types::Command;
use async_rt::CommunicationTask;
use libp2p::identity::Keypair;
use std::fmt::Debug;
use tracing::Span;

type Result<T> = std::io::Result<T>;

pub struct Connexa<T = ()> {
    #[allow(dead_code)]
    span: Span,
    keypair: Keypair,
    to_task: CommunicationTask<Command<T>>,
}

impl<T> Clone for Connexa<T> {
    fn clone(&self) -> Self {
        Self {
            span: self.span.clone(),
            keypair: self.keypair.clone(),
            to_task: self.to_task.clone(),
        }
    }
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
    pub fn swarm(&self) -> ConnexaSwarm<'_, T> {
        ConnexaSwarm::new(self)
    }

    /// Returns a handle for autonat functions
    #[cfg(feature = "autonat")]
    pub fn autonat(&self) -> ConnexaAutonat<'_, T> {
        ConnexaAutonat::new(self)
    }

    /// Returns a handle for floodsub functions
    #[cfg(feature = "floodsub")]
    pub fn floodsub(&self) -> ConnexaFloodsub<'_, T> {
        ConnexaFloodsub::new(self)
    }

    /// Returns a handle for gossipsub functions   
    #[cfg(feature = "gossipsub")]
    pub fn gossipsub(&self) -> ConnexaGossipsub<'_, T> {
        ConnexaGossipsub::new(self)
    }

    /// Returns a handle for dht functions  
    #[cfg(feature = "kad")]
    pub fn dht(&self) -> ConnexaDht<'_, T> {
        ConnexaDht::new(self)
    }

    /// Returns a handle for request-response functions
    #[cfg(feature = "request-response")]
    pub fn request_response(&self) -> ConnexaRequestResponse<'_, T> {
        ConnexaRequestResponse::new(self)
    }

    /// Returns a handle for stream functions
    #[cfg(feature = "stream")]
    pub fn stream(&self) -> ConnexaStream<'_, T> {
        ConnexaStream::new(self)
    }

    /// Returns a handle for rendezvous functions
    #[cfg(feature = "rendezvous")]
    pub fn rendezvous(&self) -> ConnexaRendezvous<'_, T> {
        ConnexaRendezvous::new(self)
    }

    /// Returns a handle to manage peer whitelist functionality
    pub fn whitelist(&self) -> ConnexaWhitelist<'_, T> {
        ConnexaWhitelist::new(self)
    }

    /// Returns a handle to manage peer blacklist functionality  
    pub fn blacklist(&self) -> ConnexaBlacklist<'_, T> {
        ConnexaBlacklist::new(self)
    }

    /// Returns a handle to the peer store
    pub fn peer_store(&self) -> ConnexaPeerstore<'_, T> {
        ConnexaPeerstore::new(self)
    }

    /// Keypair that was used during initialization
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
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
