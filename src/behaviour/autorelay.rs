mod handler;

use crate::behaviour::autorelay::handler::Out;
use crate::behaviour::dummy;
use crate::multiaddr_ext::MultiaddrExt;
use crate::prelude::swarm::derive_prelude::{ConnectionEstablished, PortUse};
use crate::prelude::swarm::{
    AddressChange, ConnectionClosed, ConnectionDenied, DialFailure, ExpiredListenAddr, FromSwarm,
    ListenerClosed, ListenerError, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
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
    connections: IndexMap<(PeerId, ConnectionId), PeerInfo>,
    static_relays: IndexMap<PeerId, IndexSet<Multiaddr>>,
    connection_reservation: IndexMap<ListenerId, (PeerId, ConnectionId)>,
    events: VecDeque<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>,
    external_addresses: ExternalAddresses,
    capacity_cleanup: Delay,
    max_reservation: NonZeroU8,
    enable_auto_relay: bool,
    backoff: Optional<Delay>,
    waker: Option<Waker>,
    remove_active_reservation_on_unsupport: bool,
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            connections: IndexMap::new(),
            static_relays: IndexMap::new(),
            connection_reservation: IndexMap::new(),
            events: VecDeque::new(),
            capacity_cleanup: Delay::new(CLEANUP_INTERVAL),
            external_addresses: ExternalAddresses::default(),
            waker: None,
            enable_auto_relay: true,
            remove_active_reservation_on_unsupport: true,
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
    pub remove_active_reservation_on_unsupport: bool,
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
            remove_active_reservation_on_unsupport: true,
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
        let total_latency: u128 = self
            .latency
            .iter()
            .map(|duration| duration.as_millis())
            .sum();
        let count = self.latency.iter().filter(|i| !i.is_zero()).count() as u128;
        if count == 0 {
            return 0;
        }
        total_latency / count
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
            remove_active_reservation_on_unsupport: config.remove_active_reservation_on_unsupport,
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

    pub fn list_static_relays(&self) -> Vec<(PeerId, Vec<Multiaddr>)> {
        self.static_relays
            .iter()
            .map(|(peer_id, addrs)| (*peer_id, Vec::from_iter(addrs.clone())))
            .collect()
    }

    pub fn get_static_relay_addrs(&self, peer_id: PeerId) -> Vec<Multiaddr> {
        let Some(addrs) = self.static_relays.get(&peer_id) else {
            return vec![];
        };

        Vec::from_iter(addrs.clone())
    }

    pub fn enable_autorelay(&mut self) {
        self.enable_auto_relay = true;
        self.meet_reservation_target(Selection::Random);
        self.events
            .push_back(ToSwarm::GenerateEvent(Event::AutoRelayEnabled));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn disable_autorelay(&mut self) {
        self.enable_auto_relay = false;
        self.events
            .push_back(ToSwarm::GenerateEvent(Event::AutoRelayDisabled));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn remove_existing_reservations(&mut self) {
        self.disable_all_reservations();
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn get_all_supported_targets(&self) -> impl Iterator<Item = (&PeerId, &ConnectionId)> {
        self.connections
            .iter()
            .filter(|(_, info)| matches!(info.relay_status, RelayStatus::Supported { .. }))
            .map(|((peer_id, connection_id), _)| (peer_id, connection_id))
    }

    fn get_pending_reservations(&self) -> impl Iterator<Item = (&PeerId, &ConnectionId)> {
        self.connections
            .iter()
            .filter(|(_, info)| {
                matches!(
                    info.relay_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Pending { .. }
                    }
                )
            })
            .map(|((peer_id, connection_id), _)| (peer_id, connection_id))
    }

    fn get_pending_reservations_count(&self) -> usize {
        self.get_pending_reservations().count()
    }

    pub fn set_peer_ping(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        duration: Duration,
    ) {
        let Some(info) = self.connections.get_mut(&(peer_id, connection_id)) else {
            return;
        };

        info.latency.rotate_left(1);
        info.latency[4] = duration;
    }

    fn get_potential_targets(&self) -> impl Iterator<Item = (&PeerId, &ConnectionId, &PeerInfo)> {
        self.connections
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
        let Some((peer_id, connection_id)) = self.connection_reservation.shift_remove(&id) else {
            tracing::error!(listener_id=%id, "could not find reservation with listener id.");
            return;
        };

        let Some(info) = self.connections.get_mut(&(peer_id, connection_id)) else {
            tracing::error!(%peer_id, %connection_id, listener_id=%id, "connection not found.");
            return;
        };

        let should_retry = match info.relay_status {
            RelayStatus::Supported {
                status: ReservationStatus::Active { .. },
            } => {
                // TODO: Determine if we should disconnect then reconnect?
                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Idle,
                };
                true
            }
            RelayStatus::Supported {
                status: ReservationStatus::Pending { .. },
            } => {
                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Idle,
                };
                true
            }
            RelayStatus::Pending
            | RelayStatus::Supported {
                status: ReservationStatus::Idle,
            }
            | RelayStatus::NotSupported => false,
        };

        if should_retry {
            self.meet_reservation_target(Selection::InOrder);
        }
    }

    fn disable_all_reservations(&mut self) {
        for (listener_id, peer_connection) in self.connection_reservation.iter() {
            let (peer_id, connection_id) = peer_connection;
            let Some(connection) = self.connections.get_mut(peer_connection) else {
                tracing::warn!(%peer_id, %connection_id, "connection not found when it should have been present. skipping");
                continue;
            };

            debug_assert!(matches!(
                connection.relay_status,
                RelayStatus::Supported {
                    status: ReservationStatus::Active { id } | ReservationStatus::Pending { id }
                } if id == *listener_id
            ));

            connection.relay_status = RelayStatus::Supported {
                status: ReservationStatus::Idle,
            };

            self.events
                .push_back(ToSwarm::RemoveListener { id: *listener_id });
            tracing::info!(%peer_id, %connection_id, ?listener_id, "removing relay listener");
        }
    }

    fn select_connection_for_reservation(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
    ) -> bool {
        if self
            .connections
            .get(&(peer_id, connection_id))
            .is_some_and(|info| {
                matches!(
                    info.relay_status,
                    RelayStatus::Supported {
                        status: ReservationStatus::Pending { .. }
                            | ReservationStatus::Active { .. }
                    }
                )
            })
        {
            tracing::warn!(%peer_id, %connection_id, "connection already has a reservation or pending reservation. skipping");
            return false;
        }

        if self.connections.is_empty() {
            tracing::warn!(%peer_id, "no connections present. removing entry");
            return false;
        }

        let Some(info) = self.connections.get_mut(&(peer_id, connection_id)) else {
            tracing::warn!(%peer_id, %connection_id, "connection not found. skipping");
            return false;
        };

        let addr_with_peer_id = match info.address.clone().with_p2p(peer_id) {
            Ok(addr) => addr,
            Err(addr) => {
                tracing::warn!(%addr, "address unexpectedly contains a different peer id than the connection");
                return false;
            }
        };

        let relay_addr = addr_with_peer_id.with(Protocol::P2pCircuit);

        let opts = ListenOpts::new(relay_addr);

        let addr = opts.address();
        let id = opts.listener_id();

        tracing::info!(%peer_id, %connection_id, %addr, ?id, "new pending reservation");

        info.relay_status = RelayStatus::Supported {
            status: ReservationStatus::Pending { id },
        };
        self.connection_reservation
            .insert(id, (peer_id, connection_id));
        self.events.push_back(ToSwarm::ListenOn { opts });
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
        {
            tracing::trace!("local node reachable. autorelay will not run");
            return;
        }

        let max = self.max_reservation.get() as usize;

        let peers_not_supported = self.connections.is_empty()
            || self
                .connections
                .iter()
                .all(|(_, info)| info.relay_status == RelayStatus::NotSupported);

        if peers_not_supported {
            if self.static_relays.is_empty() {
                tracing::warn!("no relays present.");
                self.events
                    .push_back(ToSwarm::GenerateEvent(Event::NoRelayAvailable));
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
            .connections
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

        tracing::info!(?relayed_targets, ?max, "relayed targets");

        if relayed_targets == max {
            tracing::warn!("max reservation reached. no more reservations will be made");
            return;
        }

        let pending_targets = self.get_pending_reservations_count();

        if pending_targets == max {
            tracing::warn!("pending targets reached max target.");
            return;
        }

        let max = max - relayed_targets;

        let targets = self
            .get_potential_targets()
            .map(|(peer_id, connection_id, info)| (*peer_id, *connection_id, info))
            .collect::<Vec<_>>();

        let targets_count = std::cmp::min(targets.len(), max);

        if targets_count == 0 || max == 0 {
            tracing::warn!("no potential targets to meet reservation target.");
            return;
        }

        let remaining_targets_needed = targets_count.saturating_sub(pending_targets);

        if remaining_targets_needed == 0 {
            tracing::warn!("no potential targets to meet reservation target.");
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
            if self.get_pending_reservations_count() == max {
                break;
            }

            if !self.select_connection_for_reservation(peer_id, connection_id) {
                continue;
            }
        }

        assert!(self.get_pending_reservations_count() <= max);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    NoRelayAvailable,
    AutoRelayEnabled,
    AutoRelayDisabled,
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Either<handler::Handler, dummy::DummyHandler>;
    type ToSwarm = Event;

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
                tracing::info!("local node is reachable. disabling autorelay");
                self.disable_all_reservations();
                self.backoff.take();
            } else if self.external_addresses.iter().count() == 0
                || self
                    .external_addresses
                    .iter()
                    .any(|addr| !addr.is_public() || addr.is_relayed())
            {
                tracing::info!("local node is not reachable. enabling autorelay");
                self.backoff.replace(Delay::new(BACKOFF_INTERVAL));
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

                tracing::trace!(%peer_id, %connection_id, %addr, "connection established");

                let mut info = PeerInfo {
                    address: addr,
                    relay_status: RelayStatus::Pending,
                    latency: [Duration::ZERO; 5],
                };

                // in the event that the address is from a peer going through a relay, automatically disqualify the connection
                // from being used as a potential relay since there is no support for multi-HOP
                if info.check_for_disqualifying_address() {
                    self.connections.insert((peer_id, connection_id), info);
                    return;
                }

                match self.static_relays.get(&peer_id) {
                    Some(addrs) if addrs.contains(&info.address) => {
                        // prioritize static relays so it would have a higher chance of being selected first
                        self.connections
                            .insert_before(0, (peer_id, connection_id), info);
                    }
                    _ => {
                        self.connections.insert((peer_id, connection_id), info);
                    }
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                tracing::trace!(%peer_id, %connection_id, "connection closed");
                self.connections.shift_remove(&(peer_id, connection_id));

                if let Some(listener_id) = self
                    .connection_reservation
                    .iter()
                    .find(|(_, (peer, conn_id))| peer_id.eq(peer) && connection_id.eq(conn_id))
                    .map(|(id, _)| *id)
                {
                    self.connection_reservation.shift_remove(&listener_id);
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

                self.connections.shift_remove(&(peer_id, connection_id));
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
                    .connections
                    .get_mut(&(peer_id, connection_id))
                    .expect("connection is present");

                info.address = new_addr.clone();
                tracing::trace!(%peer_id, %connection_id, %old_addr, %new_addr, "address changed");
            }
            FromSwarm::NewListenAddr(NewListenAddr { listener_id, addr }) => {
                // we only care about any new relayed address
                if !addr.iter().any(|protocol| protocol == Protocol::P2pCircuit) {
                    return;
                }

                let Some((peer_id, connection_id)) = self.connection_reservation.get(&listener_id)
                else {
                    return;
                };

                let Some(info) = self.connections.get_mut(&(*peer_id, *connection_id)) else {
                    tracing::warn!(%peer_id, %connection_id, "connection not found when it should have been present. skipping");
                    return;
                };

                let RelayStatus::Supported {
                    status: ReservationStatus::Pending { id },
                } = info.relay_status
                else {
                    tracing::warn!(%peer_id, %connection_id, "connection doesnt have a pending reservation. skipping");
                    return;
                };

                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Active { id },
                };

                tracing::info!(%peer_id, %connection_id, %addr, %id, "active reservation with relay");
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

        let Some(peer_info) = self.connections.get_mut(&(peer_id, connection_id)) else {
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
                // we should remove the reservation if its not already removed

                if self.remove_active_reservation_on_unsupport
                    && let RelayStatus::Supported {
                        status: ReservationStatus::Active { id } | ReservationStatus::Pending { id },
                    } = previous_status
                {
                    self.events.push_back(ToSwarm::RemoveListener { id });
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
            tracing::debug!("attempting to meet reservation target after node became unreachable");
            self.meet_reservation_target(Selection::InOrder);
        }

        if self.capacity_cleanup.poll_unpin(cx).is_ready() {
            if (self.events.is_empty() || self.events.len() < MAX_CAP)
                && self.events.capacity() > MAX_CAP
            {
                self.events.shrink_to_fit();
            }

            if (self.connections.is_empty() || self.connections.len() < MAX_CAP)
                && self.connections.capacity() > MAX_CAP
            {
                self.connections.shrink_to_fit();
            }

            self.capacity_cleanup.reset(CLEANUP_INTERVAL);
        }

        self.waker.replace(cx.waker().clone());

        Poll::Pending
    }
}
