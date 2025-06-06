use connexa::prelude::derive_prelude::{Endpoint, PortUse};
use connexa::prelude::{
    ConnectionDenied, ConnectionId, FromSwarm, Multiaddr, NetworkBehaviour, PeerId, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use std::collections::VecDeque;
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
pub struct Behaviour {
    ticket: u32,
    state: State,
    events: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
    waker: Option<Waker>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum State {
    On,
    Off,
}

#[derive(Debug)]
pub enum Event {
    StateChange { id: u32, old: State, new: State },
}

impl Behaviour {
    pub fn new() -> Self {
        Self {
            ticket: 0,
            state: State::Off,
            events: VecDeque::new(),
            waker: None,
        }
    }

    pub fn change_state(&mut self, new: State) -> u32 {
        let id = self.ticket.checked_add(1).unwrap();
        let old = self.state;
        self.state = new;
        self.events
            .push_back(ToSwarm::GenerateEvent(Event::StateChange { id, old, new }));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        id
    }

    pub fn get_state(&mut self) -> State {
        self.state
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = connexa::dummy::DummyHandler;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        Ok(())
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        Ok(vec![])
    }

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(connexa::dummy::DummyHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(connexa::dummy::DummyHandler)
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        _: THandlerOutEvent<Self>,
    ) {
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn poll(&mut self, cx: &mut Context) -> Poll<ToSwarm<Event, THandlerInEvent<Self>>> {
        if let Some(ev) = self.events.pop_front() {
            return Poll::Ready(ev);
        }
        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
