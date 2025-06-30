use crate::task::ConnexaTask;
use crate::types::StreamCommand;
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_stream_command(&mut self, command: StreamCommand) {
        let swarm = self.swarm.as_mut().unwrap();
        match command {
            StreamCommand::NewStream { protocol, resp } => {
                let Some(stream) = swarm.behaviour_mut().stream.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("stream protocol is not enabled")));
                    return;
                };

                let _ = resp.send(
                    stream
                        .new_control()
                        .accept(protocol)
                        .map_err(std::io::Error::other),
                );
            }
            StreamCommand::ControlHandle { resp } => {
                let Some(stream) = swarm.behaviour_mut().stream.as_mut() else {
                    let _ = resp.send(Err(std::io::Error::other("stream protocol is not enabled")));
                    return;
                };

                let _ = resp.send(Ok(stream.new_control()));
            }
        }
    }
}
