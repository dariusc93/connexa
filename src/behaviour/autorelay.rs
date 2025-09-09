mod handler;

use crate::behaviour::autorelay::handler::Out;
use crate::behaviour::dummy;
use crate::multiaddr_ext::MultiaddrExt;
use crate::prelude::swarm::derive_prelude::{ConnectionEstablished, PortUse};
use crate::prelude::swarm::{
    AddressChange, CloseConnection, ConnectionClosed, ConnectionDenied, DialFailure,
    ExpiredListenAddr, FromSwarm, ListenerClosed, ListenerError, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use crate::prelude::transport::Endpoint;
use either::Either;
use futures::FutureExt;
use futures_timer::Delay;
use indexmap::{IndexMap, IndexSet};
use libp2p::core::transport::ListenerId;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{ConnectionId, ExternalAddresses, ListenOpts, NetworkBehaviour, NewListenAddr};
use libp2p::{Multiaddr, PeerId};
use pollable_map::optional::Optional;
use rand::prelude::IteratorRandom;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU8;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

const MAX_CAP: usize = 100;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
const BACKOFF_INTERVAL: Duration = Duration::from_secs(5);

pub struct Behaviour {
    info: IndexMap<(PeerId, ConnectionId), PeerInfo>,
    static_relays: IndexMap<PeerId, IndexSet<Multiaddr>>,
    listener_to_info: IndexMap<ListenerId, (PeerId, ConnectionId)>,
    events: VecDeque<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>,
    external_addresses: ExternalAddresses,
    pending_target: IndexSet<(PeerId, ConnectionId)>,
    capacity_cleanup: Delay,
    max_reservation: NonZeroU8,
    override_autorelay: bool,
    enable_auto_relay: bool,
    backoff: Optional<Delay>,
    waker: Option<Waker>,
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            info: IndexMap::new(),
            static_relays: IndexMap::new(),
            listener_to_info: IndexMap::new(),
            events: VecDeque::new(),
            pending_target: IndexSet::new(),
            capacity_cleanup: Delay::new(CLEANUP_INTERVAL),
            external_addresses: ExternalAddresses::default(),
            override_autorelay: false,
            waker: None,
            enable_auto_relay: true,
            backoff: Optional::default(),
            max_reservation: NonZeroU8::new(2).expect("not zero"),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    pub max_reservation: NonZeroU8,
    pub enable_auto_relay: bool,
}

#[derive(Default, Debug, Clone, Copy)]
pub enum Selection {
    #[default]
    InOrder,
    Random,
    LowestLatency,
    Peer(PeerId),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_reservation: NonZeroU8::new(2).expect("not zero"),
            enable_auto_relay: true,
        }
    }
}

#[derive(Debug, Clone)]
struct PeerInfo {
    address: Multiaddr,
    relay_status: RelayStatus,
    latency: [Duration; 5],
}

impl PeerInfo {
    /// Check to see if the address is from a relay and if so, automatically disqualify the connection
    /// as we are not able to establish a reservation via multi-HOP
    pub fn check_for_disqualifying_address(&mut self) -> bool {
        match self.address.is_relayed() {
            true => {
                self.relay_status = RelayStatus::NotSupported;
                true
            }
            false => {
                self.relay_status = RelayStatus::Pending;
                false
            }
        }
    }

    pub fn average_latency(&self) -> u128 {
        let avg: u128 = self
            .latency
            .iter()
            .map(|duration| duration.as_millis())
            .sum();
        let div = self.latency.iter().filter(|i| !i.is_zero()).count() as u128;
        avg / div
    }
}

impl Hash for PeerInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayStatus {
    Supported { status: ReservationStatus },
    NotSupported,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReservationStatus {
    Idle,
    Pending { id: ListenerId },
    Active { id: ListenerId },
}

impl Behaviour {
    pub fn new_with_config(config: Config) -> Self {
        Self {
            enable_auto_relay: config.enable_auto_relay,
            max_reservation: config.max_reservation,
            ..Default::default()
        }
    }

    pub fn add_static_relay(&mut self, peer_id: PeerId, address: Multiaddr) -> bool {
        let Ok(address) = address.with_p2p(peer_id) else {
            return false;
        };

        self.static_relays
            .entry(peer_id)
            .or_default()
            .insert(address)
    }

    pub fn remove_static_relay(&mut self, peer_id: PeerId, address: Multiaddr) -> bool {
        let Ok(address) = address.with_p2p(peer_id) else {
            return false;
        };

        let Some(addrs) = self.static_relays.get_mut(&peer_id) else {
            return false;
        };

        let removed = addrs.shift_remove(&address);

        if addrs.is_empty() {
            self.static_relays.shift_remove(&peer_id);
        }

        removed
    }

    pub fn enable_autorelay(&mut self) {
        self.enable_auto_relay = true;
        self.meet_reservation_target(Selection::Random);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn disable_autorelay(&mut self) {
        self.enable_auto_relay = false;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn get_all_supported_targets(&self) -> impl Iterator<Item = (&PeerId, &ConnectionId)> {
        self.info
            .iter()
            .filter(|(_, info)| matches!(info.relay_status, RelayStatus::Supported { .. }))
            .map(|((peer_id, connection_id), _)| (peer_id, connection_id))
    }

    pub fn set_peer_ping(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        duration: Duration,
    ) {
        let Some(info) = self.info.get_mut(&(peer_id, connection_id)) else {
            return;
        };

        info.latency.rotate_left(1);
        info.latency[4] = duration;
    }

    fn get_potential_targets(&self) -> impl Iterator<Item = (&PeerId, &ConnectionId, &PeerInfo)> {
        self.info
            .iter()
            .filter(|(_, info)| {
                matches!(
                    info.relay_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Idle
                    }
                )
            })
            .map(|((peer_id, connection_id), info)| (peer_id, connection_id, info))
    }

    fn disable_reservation(&mut self, id: ListenerId) {
        let Some((peer_id, connection_id)) = self.listener_to_info.shift_remove(&id) else {
            return;
        };

        let Some(info) = self.info.get_mut(&(peer_id, connection_id)) else {
            return;
        };

        match info.relay_status {
            RelayStatus::Supported {
                status: ReservationStatus::Active { .. },
            } => {
                // TODO: Determine if we should disconnect then reconnect?
                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Idle,
                };
            }
            RelayStatus::Supported {
                status: ReservationStatus::Pending { .. },
            } => {
                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Idle,
                };
                self.pending_target.shift_remove(&(peer_id, connection_id));
            }
            RelayStatus::Pending
            | RelayStatus::Supported {
                status: ReservationStatus::Idle,
            }
            | RelayStatus::NotSupported => {}
        }
    }

    fn disable_all_reservations(&mut self) {
        let relay_listeners = self
            .listener_to_info
            .iter()
            .map(|(id, (peer_id, conn_id))| (*id, *peer_id, *conn_id))
            .collect::<Vec<_>>();

        for (listener_id, peer_id, connection_id) in relay_listeners {
            let Some(connection) = self.info.get_mut(&(peer_id, connection_id)) else {
                continue;
            };

            assert!(matches!(
                connection.relay_status,
                RelayStatus::Supported {
                    status: ReservationStatus::Active { id } | ReservationStatus::Pending { id }
                } if id == listener_id
            ));

            connection.relay_status = RelayStatus::Supported {
                status: ReservationStatus::Idle,
            };

            self.events
                .push_back(ToSwarm::RemoveListener { id: listener_id });
        }
    }

    fn select_connection_for_reservation(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
    ) -> bool {
        if self.pending_target.contains(&(peer_id, connection_id)) {
            return false;
        }

        if self.info.is_empty() {
            tracing::warn!(%peer_id, "no connections present. removing entry");
            return false;
        }

        let info = self
            .info
            .get_mut(&(peer_id, connection_id))
            .expect("connection is present");

        let addr_with_peer_id = match info.address.clone().with_p2p(peer_id) {
            Ok(addr) => addr,
            Err(addr) => {
                tracing::warn!(%addr, "address unexpectedly contains a different peer id than the connection");
                return false;
            }
        };

        let relay_addr = addr_with_peer_id.with(Protocol::P2pCircuit);

        let opts = ListenOpts::new(relay_addr);

        let id = opts.listener_id();

        info.relay_status = RelayStatus::Supported {
            status: ReservationStatus::Pending { id },
        };
        self.listener_to_info.insert(id, (peer_id, connection_id));
        self.events.push_back(ToSwarm::ListenOn { opts });
        self.pending_target.insert((peer_id, connection_id));

        true
    }

    fn meet_reservation_target(&mut self, selection: Selection) {
        if !self.enable_auto_relay {
            return;
        }

        // check to determine if there is a public external address that could possibly let us know the node
        // is reachable
        if self
            .external_addresses
            .iter()
            .any(|addr| addr.is_public() && !addr.is_relayed())
            && !self.override_autorelay
        {
            return;
        }

        let max = self.max_reservation.get() as usize;

        let peers_not_supported = self.info.is_empty()
            || self
                .info
                .iter()
                .all(|(_, info)| info.relay_status == RelayStatus::NotSupported);

        if peers_not_supported {
            if self.static_relays.is_empty() {
                // TODO: Emit an event informing swarm about being in need of relays?
                // however this would require separate functions to add relays to the autorelay state and possibly confirm if theres any existing connections
                return;
            }
            for (peer_id, addrs) in self.static_relays.iter() {
                let opts = DialOpts::peer_id(*peer_id)
                    .addresses(Vec::from_iter(addrs.clone()))
                    .build();
                self.events.push_back(ToSwarm::Dial { opts });
            }
            return;
        }

        let relayed_targets = self
            .info
            .iter()
            .filter(|(_, info)| {
                matches!(
                    info.relay_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Active { .. }
                    }
                )
            })
            .count();

        if relayed_targets == max {
            return;
        }

        let pending_targets = self
            .info
            .iter()
            .filter(|(_, info)| {
                matches!(
                    info.relay_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Pending { .. }
                    }
                )
            })
            .count();

        if pending_targets == max {
            return;
        }

        let max = max - relayed_targets;

        let targets = self
            .get_potential_targets()
            .map(|(peer_id, connection_id, info)| (*peer_id, *connection_id, info))
            .collect::<Vec<_>>();

        let targets_count = std::cmp::min(targets.len(), max);

        if targets_count == 0 || max == 0 {
            return;
        }

        let remaining_targets_needed = targets_count
            .checked_sub(self.pending_target.len())
            .unwrap_or_default();

        if remaining_targets_needed == 0 {
            return;
        }

        let new_targets = match selection {
            Selection::InOrder => targets
                .into_iter()
                .map(|(peer_id, connection_id, _)| (peer_id, connection_id))
                .take(remaining_targets_needed)
                .collect::<Vec<_>>(),
            Selection::Random => {
                let mut rng = rand::thread_rng();
                targets
                    .into_iter()
                    .map(|(peer_id, connection_id, _)| (peer_id, connection_id))
                    .choose_multiple(&mut rng, remaining_targets_needed)
            }
            Selection::Peer(peer_id) => targets
                .into_iter()
                .filter(|(id, _, _)| *id == peer_id)
                .map(|(peer_id, connection_id, _)| (peer_id, connection_id))
                .collect::<Vec<_>>(),
            Selection::LowestLatency => {
                let mut targets = targets;
                targets.sort_by(|(_, _, info1), (_, _, info2)| {
                    let avg1 = info1.average_latency();
                    let avg2 = info2.average_latency();
                    avg1.cmp(&avg2)
                });

                targets
                    .into_iter()
                    .take(remaining_targets_needed)
                    .map(|(peer_id, connection_id, _)| (peer_id, connection_id))
                    .collect::<Vec<_>>()
            }
        };

        for (peer_id, connection_id) in new_targets {
            if self.pending_target.len() == max {
                break;
            }

            if !self.select_connection_for_reservation(peer_id, connection_id) {
                continue;
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

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        // To prevent providing addresses from active connections, we will only focus on addresses added here that are considered to be fixed/static relays.
        let Some(addrs) = maybe_peer
            .and_then(|peer_id| self.static_relays.get(&peer_id).cloned())
            .map(Vec::from_iter)
        else {
            return Ok(vec![]);
        };

        Ok(addrs)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let change = self.external_addresses.on_swarm_event(&event);
        if change {
            if self
                .external_addresses
                .iter()
                .any(|addr| addr.is_public() && !addr.is_relayed())
            {
                self.override_autorelay = true;
                self.disable_all_reservations();
                self.backoff.take();
            } else if self.external_addresses.iter().count() == 0
                || self
                    .external_addresses
                    .iter()
                    .any(|addr| !addr.is_public() || addr.is_relayed())
            {
                self.backoff.replace(Delay::new(BACKOFF_INTERVAL));
                self.override_autorelay = false;
            }
            return;
        }

        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            }) => {
                let addr = endpoint.get_remote_address().clone();

                let mut info = PeerInfo {
                    address: addr,
                    relay_status: RelayStatus::Pending,
                    latency: [Duration::ZERO; 5],
                };

                // in the event that the address is from a peer going through a relay, automatically disqualify the connection
                // from being used as a potential relay since there is no support for multi-HOP
                if info.check_for_disqualifying_address() {
                    self.info.insert((peer_id, connection_id), info);
                } else {
                    match self.static_relays.get(&peer_id) {
                        Some(addrs) if addrs.contains(&info.address) => {
                            // prioritize static relays so it would have a higher chance of being selected first
                            self.info.insert_before(0, (peer_id, connection_id), info);
                        }
                        _ => {
                            self.info.insert((peer_id, connection_id), info);
                        }
                    }
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                self.info.shift_remove(&(peer_id, connection_id));

                if let Some(listener_id) = self
                    .listener_to_info
                    .iter()
                    .find(|(_, (peer, conn_id))| peer_id.eq(peer) && connection_id.eq(conn_id))
                    .map(|(id, _)| *id)
                {
                    self.listener_to_info.shift_remove(&listener_id);
                }
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id,
                connection_id,
                error,
            }) => {
                tracing::error!(maybe_peer = ?peer_id, %connection_id, %error, "failed to dial peer");

                let Some(peer_id) = peer_id else {
                    return;
                };

                self.info.shift_remove(&(peer_id, connection_id));
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

                let info = self
                    .info
                    .get_mut(&(peer_id, connection_id))
                    .expect("connection is present");

                info.address = new_addr.clone();
            }
            FromSwarm::NewListenAddr(NewListenAddr { listener_id, addr }) => {
                // we only care about any new relayed address
                if !addr.iter().any(|protocol| protocol == Protocol::P2pCircuit) {
                    return;
                }

                let Some((peer_id, connection_id)) = self.listener_to_info.get(&listener_id) else {
                    return;
                };

                let Some(info) = self.info.get_mut(&(*peer_id, *connection_id)) else {
                    return;
                };

                let RelayStatus::Supported {
                    status: ReservationStatus::Pending { id },
                } = info.relay_status
                else {
                    return;
                };

                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Active { id },
                };

                debug_assert!(
                    self.pending_target
                        .shift_remove(&(*peer_id, *connection_id))
                );
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

        let Some(peer_info) = self.info.get_mut(&(peer_id, connection_id)) else {
            return;
        };

        match event {
            Out::Supported => {
                peer_info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Idle,
                };
                self.meet_reservation_target(Selection::InOrder);
            }
            Out::Unsupported => {
                let previous_status = peer_info.relay_status;
                peer_info.relay_status = RelayStatus::NotSupported;
                // if there is a change in protocol support during an active reservation,
                // we should disconnect to remove the reservation
                if matches!(
                    previous_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Active { .. }
                    }
                ) {
                    self.events.push_back(ToSwarm::CloseConnection {
                        peer_id,
                        connection: CloseConnection::One(connection_id),
                    });

                    // if infos.len() == 1 {
                    //     // TODO: Determine if we should reconnect if this is the only connection
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

        if self.backoff.poll_unpin(cx).is_ready() {
            self.meet_reservation_target(Selection::InOrder);
        }

        if self.capacity_cleanup.poll_unpin(cx).is_ready() {
            if (self.events.is_empty() || self.events.len() < MAX_CAP)
                && self.events.capacity() > MAX_CAP
            {
                self.events.shrink_to_fit();
            }

            if (self.info.is_empty() || self.info.len() < MAX_CAP) && self.info.capacity() > MAX_CAP
            {
                self.info.shrink_to_fit();
            }

            self.capacity_cleanup.reset(CLEANUP_INTERVAL);
        }

        self.waker.replace(cx.waker().clone());

        Poll::Pending
    }
}
