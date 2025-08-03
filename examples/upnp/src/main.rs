use connexa::behaviour::BehaviourEvent;
use connexa::prelude::swarm::SwarmEvent;
use connexa::prelude::upnp::Event as UpnpEvent;
use connexa::prelude::{DefaultConnexaBuilder, Protocol};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_tcp()
        .with_upnp()
        .set_swarm_event_callback(|event| match event {
            SwarmEvent::NewListenAddr { address: addr, .. } => {
                println!("New listen address: {addr}")
            }
            SwarmEvent::Behaviour(BehaviourEvent::Upnp(event)) => match event {
                UpnpEvent::NewExternalAddr(addr) => println!("New external address: {addr}"),
                UpnpEvent::ExpiredExternalAddr(addr) => {
                    println!("Expired external address: {addr}")
                }
                UpnpEvent::GatewayNotFound => println!("Gateway not found"),
                UpnpEvent::NonRoutableGateway => println!("Gateway is not routable"),
            },
            _ => {}
        })
        .build()?;

    let id = connexa
        .swarm()
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
        .await?;

    let addrs = connexa.swarm().get_listening_addresses(id).await?;

    let peer_id = connexa.keypair().public().to_peer_id();

    for addr in addrs {
        let addr = addr.with(Protocol::P2p(peer_id));
        connexa.swarm().add_external_address(addr).await?;
    }

    tokio::signal::ctrl_c().await.unwrap();

    Ok(())
}
