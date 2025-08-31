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
use rand::prelude::IteratorRandom;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU8;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

const MAX_CAP: usize = 100;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

pub struct Behaviour {
    info: IndexMap<PeerId, VecDeque<PeerInfo>>,
    static_relays: IndexMap<PeerId, IndexSet<Multiaddr>>,
    listener_to_info: IndexMap<ListenerId, (PeerId, ConnectionId)>,
    events: VecDeque<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>,
    external_addresses: ExternalAddresses,
    pending_target: IndexSet<PeerId>,
    capacity_cleanup: Delay,
    max_reservation: NonZeroU8,
    override_autorelay: bool,
    enable_auto_relay: bool,
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
    connection_id: ConnectionId,
    address: Multiaddr,
    relay_status: RelayStatus,
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
}

impl Hash for PeerInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.connection_id.hash(state);
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

    pub fn get_all_supported_targets(&self) -> impl Iterator<Item = &PeerId> {
        self.info
            .iter()
            .filter(|(_, infos)| {
                infos
                    .iter()
                    .any(|info| matches!(info.relay_status, RelayStatus::Supported { .. }))
            })
            .map(|(peer_id, _)| peer_id)
    }

    pub fn get_potential_targets(&self) -> impl Iterator<Item = &PeerId> {
        self.info
            .iter()
            .filter(|(_, infos)| {
                infos.iter().any(|info| {
                    matches!(
                        info.relay_status,
                        RelayStatus::Supported {
                            status: ReservationStatus::Idle
                        }
                    )
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

        let Some(info) = connections
            .iter_mut()
            .find(|info| info.connection_id == connection_id)
        else {
            return;
        };

        match info.relay_status {
            RelayStatus::Supported {
                status: ReservationStatus::Active { .. },
            } => {
                // TODO: Determine if we should disconnect then reconnect?
            }
            RelayStatus::Supported {
                status: ReservationStatus::Pending { .. },
            } => {
                self.pending_target.shift_remove(&peer_id);
            }
            RelayStatus::Supported {
                status: ReservationStatus::Idle,
            }
            | RelayStatus::NotSupported
            | RelayStatus::Pending => {}
        }

        info.relay_status = RelayStatus::NotSupported;
    }

    fn select_connection_for_reservation(&mut self, peer_id: &PeerId) -> bool {
        if self.pending_target.contains(peer_id) {
            return false;
        }

        let Some(connections) = self.info.get_mut(peer_id) else {
            return false;
        };

        if connections.is_empty() {
            self.info.shift_remove(peer_id);
            tracing::warn!(%peer_id, "no connections present. removing entry");
            return false;
        }

        let mut rng = rand::thread_rng();

        let info = connections
            .iter_mut()
            .choose(&mut rng)
            .expect("connection is present");

        let addr_with_peer_id = match info.address.clone().with_p2p(*peer_id) {
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
        self.listener_to_info
            .insert(id, (*peer_id, info.connection_id));
        self.events.push_back(ToSwarm::ListenOn { opts });
        self.pending_target.insert(*peer_id);

        true
    }

    #[allow(clippy::manual_saturating_arithmetic)]
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
            return;
        }

        let max = self.max_reservation.get() as usize;

        // TODO: check to determine if we have any active connections and if not, dial any static relays and let it be handled internally
        let peers_not_supported = self.info.is_empty()
            || self
                .info
                .iter()
                .filter(|(_, infos)| {
                    infos
                        .iter()
                        .all(|info| info.relay_status == RelayStatus::NotSupported)
                })
                .count()
                == 0;

        if peers_not_supported {
            if self.static_relays.is_empty() {
                // TODO: Emit an event informing swarm about being in need of relays?
                // however this would require separate functions to add relays to the autorelay state and possibly confirm if theres any existing connections
                return;
            }
            for (peer_id, addrs) in self.static_relays.iter() {
                for addr in addrs.iter().cloned() {
                    let opts = DialOpts::peer_id(*peer_id).addresses(vec![addr]).build();
                    self.events.push_back(ToSwarm::Dial { opts });
                }
            }
            return;
        }

        let relayed_targets = self
            .info
            .iter()
            .filter(|(_, info)| {
                info.iter().any(|info| {
                    matches!(
                        info.relay_status,
                        RelayStatus::Supported {
                            status: ReservationStatus::Active { .. }
                        }
                    )
                })
            })
            .count();

        if relayed_targets == max {
            return;
        }

        let targets = self.get_potential_targets().copied().collect::<Vec<_>>();

        let pending_target_len = self.pending_target.len();

        if pending_target_len >= max {
            return;
        }

        let targets_count = std::cmp::min(targets.len(), max);

        if targets_count == 0 {
            return;
        }

        let remaining_targets_needed = targets_count
            .checked_sub(self.pending_target.len())
            .unwrap_or_default();

        if remaining_targets_needed == 0 {
            return;
        }

        let mut rng = rand::thread_rng();

        let new_targets = match selection {
            Selection::InOrder => targets
                .into_iter()
                .take(remaining_targets_needed)
                .collect::<Vec<_>>(),
            Selection::Random => targets
                .into_iter()
                .choose_multiple(&mut rng, remaining_targets_needed),
            Selection::Peer(peer_id) => targets.into_iter().filter(|&id| id == peer_id).collect(),
            Selection::LowestLatency => {
                unimplemented!()
            }
        };

        for peer_id in new_targets {
            if !self.select_connection_for_reservation(&peer_id) {
                continue;
            }

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

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let Some(addrs) = maybe_peer.and_then(|peer_id| self.static_relays.get_mut(&peer_id))
        else {
            return Ok(vec![]);
        };

        // To prevent providing addresses from active connections, we will only focus on addresses added here that are considered to be fixed/static relays.
        let addrs = addrs.iter().cloned().collect::<Vec<_>>();

        Ok(addrs)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let _change = self.external_addresses.on_swarm_event(&event);
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            }) => {
                let infos = self.info.entry(peer_id).or_default();
                let addr = endpoint.get_remote_address().clone();

                let mut info = PeerInfo {
                    connection_id,
                    address: addr,
                    relay_status: RelayStatus::Pending,
                };

                // in the event that the address is from a peer going through a relay, automatically disqualify the connection
                // from being used as a potential relay since there is no support for multi-HOP
                if info.check_for_disqualifying_address() {
                    infos.push_back(info);
                } else {
                    match self.static_relays.get(&peer_id) {
                        Some(addrs) if addrs.contains(&info.address) => {
                            // prioritize static relays so it would have a higher chance of being selected first
                            infos.push_front(info);
                        }
                        _ => {
                            infos.push_back(info);
                        }
                    }
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                let Some(infos) = self.info.get_mut(&peer_id) else {
                    return;
                };

                infos
                    .iter()
                    .position(|info| info.connection_id == connection_id)
                    .map(|index| infos.remove(index));

                // TODO: Determine if we should remove it here or leave it for the listener events to handle its removal
                if let Some(listener_id) = self
                    .listener_to_info
                    .iter()
                    .find(|(_, (peer, conn_id))| peer_id.eq(peer) && connection_id.eq(conn_id))
                    .map(|(id, _)| *id)
                {
                    self.listener_to_info.shift_remove(&listener_id);
                }

                if infos.is_empty() {
                    self.info.shift_remove(&peer_id);
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

                let Some(infos) = self.info.get_mut(&peer_id) else {
                    return;
                };

                infos
                    .iter()
                    .position(|info| info.connection_id == connection_id)
                    .map(|index| infos.remove(index));
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
                    .get_mut(&peer_id)
                    .and_then(|infos| {
                        infos
                            .iter()
                            .position(|info| info.connection_id == connection_id)
                            .and_then(|index| infos.get_mut(index))
                    })
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

                let Some(infos) = self.info.get_mut(peer_id) else {
                    return;
                };

                let info = infos
                    .iter()
                    .position(|info| info.connection_id == *connection_id)
                    .and_then(|index| infos.get_mut(index))
                    .expect("connection is present");

                let RelayStatus::Supported {
                    status: ReservationStatus::Pending { id },
                } = info.relay_status
                else {
                    return;
                };

                info.relay_status = RelayStatus::Supported {
                    status: ReservationStatus::Active { id },
                };

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

        let Some(infos) = self.info.get_mut(&peer_id) else {
            return;
        };

        let peer_info = infos
            .iter()
            .position(|info| info.connection_id == connection_id)
            .and_then(|index| infos.get_mut(index))
            .expect("connection is present");

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
                if matches!(previous_status, RelayStatus::Supported { .. }) {
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
