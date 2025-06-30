use crate::task::ConnexaTask;
use crate::types::RequestResponseCommand;
use libp2p::swarm::NetworkBehaviour;
use std::fmt::Debug;

impl<X, C: NetworkBehaviour, T> ConnexaTask<X, C, T>
where
    X: Default + Send + 'static,
    C: Send,
    C::ToSwarm: Debug,
{
    pub fn process_request_response_command(&mut self, command: RequestResponseCommand) {
        let swarm = self.swarm.as_mut().unwrap();
        match command {
            RequestResponseCommand::SendRequests {
                protocol,
                peers,
                request,
                resp,
            } => {
                let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "request response protocol is not enabled",
                    )));
                    return;
                };

                let st = rr.send_requests(peers, request);
                let _ = resp.send(Ok(st));
            }
            RequestResponseCommand::SendRequest {
                protocol,
                peer_id,
                request,
                resp,
            } => {
                let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "request response protocol is not enabled",
                    )));
                    return;
                };

                let fut = rr.send_request(peer_id, request);
                let _ = resp.send(Ok(fut));
            }
            RequestResponseCommand::SendResponse {
                protocol,
                peer_id,
                request_id,
                response,
                resp,
            } => {
                let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "request response protocol is not enabled",
                    )));
                    return;
                };

                let ret = rr.send_response(peer_id, request_id, response);

                let _ = resp.send(ret);
            }
            RequestResponseCommand::ListenForRequests { protocol, resp } => {
                let Some(rr) = swarm.behaviour_mut().request_response(protocol) else {
                    let _ = resp.send(Err(std::io::Error::other(
                        "request response protocol is not enabled",
                    )));
                    return;
                };
                let rx = rr.subscribe();
                let _ = resp.send(Ok(rx));
            }
        }
    }
}
