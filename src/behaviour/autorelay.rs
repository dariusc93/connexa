mod handler;

use crate::behaviour::autorelay::handler::Out;
use crate::behaviour::dummy;
use crate::multiaddr_ext::MultiaddrExt;
use crate::prelude::swarm::derive_prelude::{ConnectionEstablished, PortUse};
use crate::prelude::swarm::{
    AddressChange, CloseConnection, ConnectionClosed, ConnectionDenied, ExpiredListenAddr,
    FromSwarm, ListenerClosed, ListenerError, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use crate::prelude::transport::Endpoint;
use either::Either;
use indexmap::{IndexMap, IndexSet};
use libp2p::core::transport::ListenerId;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{ConnectionId, ListenOpts, NetworkBehaviour, NewListenAddr};
use libp2p::{Multiaddr, PeerId};
use rand::prelude::IteratorRandom;
use std::collections::VecDeque;
use std::task::{Context, Poll, Waker};

#[derive(Default)]
pub struct Behaviour {
    config: Config,
    info: IndexMap<PeerId, IndexMap<ConnectionId, PeerInfo>>,
    listener_to_info: IndexMap<ListenerId, (PeerId, ConnectionId)>,
    _static_relays: IndexSet<Multiaddr>,
    events: VecDeque<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>,
    pending_target: IndexSet<PeerId>,
    waker: Option<Waker>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    pub max_reservation: Option<u8>,
    pub auto_reservation: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_reservation: None,
            auto_reservation: true,
        }
    }
}

#[derive(Debug)]
struct PeerInfo {
    address: Multiaddr,
    relay_supported: bool,
    reservation_status: ReservationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReservationStatus {
    Pending { id: ListenerId },
    Active { id: ListenerId },
    None,
}

// impl ReservationStatus {
//     pub fn disabled(self) -> bool {
//         self == ReservationStatus::None
//     }
// }

impl Behaviour {
    pub fn new_with_config(config: Config) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    // pub fn add_relay(&mut self, address: Multiaddr) -> bool {
    //     self.static_relays.insert(address)
    // }
    //
    // pub fn remove_relay(&mut self, address: Multiaddr) -> bool {
    //     self.static_relays.shift_remove(&address)
    // }

    pub fn enable_autorelay(&mut self) {
        self.config.auto_reservation = true;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn disable_autorelay(&mut self) {
        self.config.auto_reservation = false;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn get_all_supported_targets(&self) -> impl Iterator<Item = &PeerId> {
        self.info
            .iter()
            .filter(|(_, infos)| infos.iter().any(|(_, info)| info.relay_supported))
            .map(|(peer_id, _)| peer_id)
    }

    pub fn get_supported_targets(&self) -> impl Iterator<Item = &PeerId> {
        self.info
            .iter()
            .filter(|(_, infos)| {
                infos.iter().any(|(_, info)| {
                    info.relay_supported && info.reservation_status == ReservationStatus::None
                })
            })
            .map(|(peer_id, _)| peer_id)
    }

    fn disable_reservation(&mut self, id: ListenerId) {
        let Some((peer_id, connection_id)) = self.listener_to_info.shift_remove(&id) else {
            return;
        };

        let Some(connections) = self.info.get_mut(&peer_id) else {
            return;
        };

        let Some(info) = connections.get_mut(&connection_id) else {
            return;
        };

        match info.reservation_status {
            ReservationStatus::Active { .. } => {
                // TODO: Determine if we should disconnect then reconnect?
            }
            ReservationStatus::Pending { .. } => {
                self.pending_target.shift_remove(&peer_id);
            }
            ReservationStatus::None => {
                // FIXME: Unreachable?
            }
        }

        info.reservation_status = ReservationStatus::None;
    }

    #[allow(clippy::manual_saturating_arithmetic)]
    fn meet_reservation_target(&mut self) {
        if !self.config.auto_reservation {
            return;
        }

        let max = self.config.max_reservation.unwrap_or(2) as usize;

        // if max target reservation is 0, this would be no different from disabling auto reservation
        // to prevent a DoS due to acquiring multiple reservations from an unbound number of relays
        if max == 0 {
            return;
        }

        let relayed_targets = self
            .info
            .iter()
            .filter(|(_, info)| {
                info.iter().any(|(_, info)| {
                    info.relay_supported
                        && matches!(info.reservation_status, ReservationStatus::Active { .. })
                })
            })
            .count();

        if relayed_targets == max as usize {
            return;
        }

        let targets = self.get_supported_targets().copied().collect::<Vec<_>>();

        let pending_target_len = self.pending_target.len();

        if pending_target_len >= max {
            return;
        }

        debug_assert!(pending_target_len < max);

        let targets_count = targets.len();

        if targets_count == 0 {
            return;
        }

        let mut rng = rand::thread_rng();

        let remaining_targets_needed = targets_count
            .checked_sub(self.pending_target.len())
            .unwrap_or_default();

        if remaining_targets_needed == 0 {
            return;
        }

        let targets = targets
            .into_iter()
            .choose_multiple(&mut rng, remaining_targets_needed);

        for peer_id in targets {
            let connections = self.info.get_mut(&peer_id).expect("peer entry is valud");

            let (connection_id, info) = connections
                .iter_mut()
                .choose(&mut rng)
                .expect("connection is present");

            assert_eq!(info.reservation_status, ReservationStatus::None);

            let addr_with_peer_id = match info.address.clone().with_p2p(peer_id) {
                Ok(addr) => addr,
                Err(addr) => {
                    tracing::warn!(%addr, "address unexpectedly contains a different peer id than the connection");
                    return;
                }
            };

            let relay_addr = addr_with_peer_id.with(Protocol::P2pCircuit);

            let opts = ListenOpts::new(relay_addr);

            let id = opts.listener_id();

            info.reservation_status = ReservationStatus::Pending { id };
            self.listener_to_info.insert(id, (peer_id, *connection_id));
            self.events.push_back(ToSwarm::ListenOn { opts });
            self.pending_target.insert(peer_id);
            if self.pending_target.len() == max {
                break;
            }
        }

        assert!(self.pending_target.len() <= max);
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Either<handler::Handler, dummy::DummyHandler>;
    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if local_addr.is_relayed() {
            Ok(Either::Right(dummy::DummyHandler))
        } else {
            Ok(Either::Left(handler::Handler::default()))
        }
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if addr.is_relayed() {
            Ok(Either::Right(dummy::DummyHandler))
        } else {
            Ok(Either::Left(handler::Handler::default()))
        }
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            }) => {
                let connections = self.info.entry(peer_id).or_default();
                let addr = endpoint.get_remote_address().clone();
                let info = PeerInfo {
                    address: addr,
                    relay_supported: false,
                    reservation_status: ReservationStatus::None,
                };
                connections.insert(connection_id, info);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                let Some(connections) = self.info.get_mut(&peer_id) else {
                    return;
                };

                let _info = connections
                    .shift_remove(&connection_id)
                    .expect("connection was present");

                // if matches!(info.reservation_status, ReservationStatus::Pending { .. }) {
                //     self.pending_target = self.pending_target.checked_sub(1).unwrap_or_default();
                // }

                if connections.is_empty() {
                    self.info.shift_remove(&peer_id);
                }
            }
            FromSwarm::AddressChange(AddressChange {
                peer_id,
                connection_id,
                old,
                new,
            }) => {
                let old_addr = old.get_remote_address();
                let new_addr = new.get_remote_address();

                debug_assert!(old_addr != new_addr);

                let Some(connections) = self.info.get_mut(&peer_id) else {
                    return;
                };

                let Some(info) = connections.get_mut(&connection_id) else {
                    return;
                };

                info.address = new_addr.clone();
            }
            FromSwarm::NewListenAddr(NewListenAddr { listener_id, addr }) => {
                let Some((peer_id, connection_id)) = self.listener_to_info.get(&listener_id) else {
                    return;
                };

                if !addr.iter().any(|protocol| protocol == Protocol::P2pCircuit) {
                    return;
                }

                let Some(connections) = self.info.get_mut(peer_id) else {
                    return;
                };

                let Some(info) = connections.get_mut(connection_id) else {
                    return;
                };

                let ReservationStatus::Pending { id } = info.reservation_status else {
                    return;
                };

                info.reservation_status = ReservationStatus::Active { id };

                debug_assert!(self.pending_target.shift_remove(peer_id));
            }
            FromSwarm::ExpiredListenAddr(ExpiredListenAddr { listener_id, .. })
            | FromSwarm::ListenerError(ListenerError { listener_id, .. })
            | FromSwarm::ListenerClosed(ListenerClosed { listener_id, .. }) => {
                self.disable_reservation(listener_id)
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let Either::Left(event) = event;

        let Some(connections) = self.info.get_mut(&peer_id) else {
            return;
        };

        let Some(peer_info) = connections.get_mut(&connection_id) else {
            return;
        };

        match event {
            Out::Supported => {
                peer_info.relay_supported = true;
                self.meet_reservation_target();
            }
            Out::Unsupported => {
                peer_info.relay_supported = false;
                // if there is a change in protocol support during an active reservation,
                // we should disconnect to remove the reservation
                if peer_info.reservation_status != ReservationStatus::None {
                    self.events.push_back(ToSwarm::CloseConnection {
                        peer_id,
                        connection: CloseConnection::One(connection_id),
                    });

                    // TODO: Determine if we should reconnect if this is the only connection
                    // if connections.len() == 1 {
                    //     let addr = peer_info.address.clone();
                    //     let opts = DialOpts::peer_id(peer_id).addresses(vec![addr]).build();
                    //     self.events.push_back(ToSwarm::Dial { opts });
                    // }
                }
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        self.waker.replace(cx.waker().clone());

        Poll::Pending
    }
}
