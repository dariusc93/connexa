use crate::behaviour::{Event, State};
use connexa::builder::ConnexaBuilder;
use futures::channel::oneshot;
use std::collections::HashMap;

mod behaviour;

#[derive(Debug, Default)]
struct Context {
    pending_state_change: HashMap<u32, oneshot::Sender<()>>,
}

enum Command {
    ChangeState {
        state: State,
        resp: oneshot::Sender<()>,
    },
    GetState {
        resp: oneshot::Sender<State>,
    },
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let connexa = ConnexaBuilder::<Context, behaviour::Behaviour, Command>::new_identity()
        .with_custom_behaviour(|_| behaviour::Behaviour::new())
        .set_custom_event_callback(|_, context, event| {
            let Event::StateChange { id, old, new } = event;

            assert_ne!(old, new);

            if let Some(ch) = context.pending_state_change.remove(&id) {
                let _ = ch.send(());
            }
        })
        .set_custom_task_callback(|swarm, task, command| {
            let Some(custom_behaviour) = swarm.behaviour_mut().custom.as_mut() else {
                return;
            };

            match command {
                Command::ChangeState { state, resp } => {
                    let id = custom_behaviour.change_state(state);
                    task.pending_state_change.insert(id, resp);
                }
                Command::GetState { resp } => {
                    let state = custom_behaviour.get_state();
                    let _ = resp.send(state);
                }
            }
        })
        .start()?;

    {
        let (tx, rx) = oneshot::channel();
        connexa
            .send_custom_event(Command::GetState { resp: tx })
            .await?;

        let state = rx.await.unwrap();
        assert_eq!(state, State::Off);
    }

    {
        let (tx, rx) = oneshot::channel();
        connexa
            .send_custom_event(Command::ChangeState {
                state: State::On,
                resp: tx,
            })
            .await?;

        rx.await.unwrap();
    }

    {
        let (tx, rx) = oneshot::channel();
        connexa
            .send_custom_event(Command::GetState { resp: tx })
            .await?;

        let state = rx.await.unwrap();
        assert_eq!(state, State::On);
    }

    Ok(())
}
