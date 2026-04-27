use crate::handle::Connexa;
use crate::types::RelayServerCommand;
use futures::TryFutureExt;
use libp2p::relay::Status as RelayServerStatus;

#[derive(Copy, Clone)]
pub struct ConnexaRelayServer<'a, T = ()> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaRelayServer<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }

    pub async fn change_status(
        &self,
        status: impl Into<Option<RelayServerStatus>>,
    ) -> std::io::Result<()> {
        let (tx, rx) = futures::channel::oneshot::channel();
        let status = status.into();
        self.connexa
            .to_task
            .clone()
            .send(RelayServerCommand::StatusChanged { status, resp: tx }.into())
            .await?;
        rx.await.map_err(std::io::Error::other)?
    }
}
