use crate::prelude::NetworkBehaviour;
use crate::task::ConnexaTask;
use libp2p::autonat::v1::Event as AutonatV1Event;
use libp2p::autonat::v2::client::Event as AutonatV2ClientEvent;
use libp2p::autonat::v2::server::Event as AutonatV2ServerEvent;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_autonat_v1_event(&mut self, event: AutonatV1Event) {
        match event {
            AutonatV1Event::InboundProbe(_) => {}
            AutonatV1Event::OutboundProbe(_) => {}
            AutonatV1Event::StatusChanged { old, new } => {
                tracing::info!(old = ?old, new = ?new, "nat status changed");
            }
        }
    }

    pub fn process_autonat_v2_client_event(&mut self, event: AutonatV2ClientEvent) {
        let AutonatV2ClientEvent {
            tested_addr,
            bytes_sent,
            server,
            result,
        } = event;
        match result {
            Ok(_) => {
                tracing::info!(%tested_addr, %bytes_sent, %server, "autonat v2 response succeed");
            }
            Err(e) => {
                tracing::info!(%tested_addr, %bytes_sent, %server, error=%e, "autonat v2 response error");
            }
        }
    }

    pub fn process_autonat_v2_server_event(&mut self, event: AutonatV2ServerEvent) {
        let AutonatV2ServerEvent {
            all_addrs,
            tested_addr,
            client,
            data_amount,
            result,
        } = event;
        match result {
            Ok(_) => {
                tracing::info!(%tested_addr, ?all_addrs, %client, %data_amount, "autonat v2 response succeed");
            }
            Err(e) => {
                tracing::info!(%tested_addr, ?all_addrs, %client, %data_amount, error=%e, "autonat v2 response error");
            }
        }
    }
}
