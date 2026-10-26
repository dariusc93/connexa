#![cfg(feature = "gossipsub")]

mod common;

use connexa::error::{Error, Protocol, gossipsub};
use connexa::prelude::{GossipsubEvent, GossipsubMessage};
use futures::stream::BoxStream;

async fn next_message(events: &mut BoxStream<'static, GossipsubEvent>) -> GossipsubMessage {
    let event = common::next_matching(events, |event| {
        matches!(event, GossipsubEvent::Message { .. })
    })
    .await;
    let GossipsubEvent::Message { message } = event else {
        unreachable!();
    };
    message
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn deliver_messages_to_topic_listeners() {
    common::timeout(async {
        let (sender, sender_id, _) =
            common::spawn_connexa_with_config(|builder| builder.with_gossipsub()).await;
        let (receiver, receiver_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_gossipsub()).await;
        let mut first_subscriptions = sender.gossipsub().listener("first").await.unwrap();
        let mut second_subscriptions = sender.gossipsub().listener("second").await.unwrap();
        let mut first_events = receiver.gossipsub().listener("first").await.unwrap();
        let mut another_listener = receiver.gossipsub().listener("first").await.unwrap();
        let mut second_events = receiver.gossipsub().listener("second").await.unwrap();

        sender.swarm().dial(addr).await.unwrap();
        for topic in ["first", "second"] {
            sender.gossipsub().subscribe(topic).await.unwrap();
            receiver.gossipsub().subscribe(topic).await.unwrap();
        }
        for events in [&mut first_subscriptions, &mut second_subscriptions] {
            common::next_matching(events, |event| {
                matches!(event, GossipsubEvent::Subscribed { peer_id } if *peer_id == receiver_id)
            })
            .await;
        }
        assert_eq!(
            sender.gossipsub().peers("first").await.unwrap(),
            vec![receiver_id]
        );

        sender
            .gossipsub()
            .publish("first", "first message")
            .await
            .unwrap();
        sender
            .gossipsub()
            .publish("second", "second message")
            .await
            .unwrap();

        let first = next_message(&mut first_events).await;
        assert_eq!(first.source, Some(sender_id));
        assert_eq!(first.propagated_source, sender_id);
        assert_eq!(first.data.as_ref(), b"first message");
        assert_eq!(next_message(&mut another_listener).await, first);
        let second = next_message(&mut second_events).await;
        assert_eq!(second.source, Some(sender_id));
        assert_eq!(second.data.as_ref(), b"second message");

        drop(another_listener);
        sender
            .gossipsub()
            .publish("first", "next message")
            .await
            .unwrap();
        assert_eq!(
            next_message(&mut first_events).await.data.as_ref(),
            b"next message"
        );
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn track_remote_subscriptions() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_gossipsub()).await;
        let (peer, peer_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_gossipsub()).await;
        let mut events = node.gossipsub().listener("topic").await.unwrap();
        assert!(node.gossipsub().peers("topic").await.unwrap().is_empty());
        node.swarm().dial(addr).await.unwrap();

        peer.gossipsub().subscribe("topic").await.unwrap();
        common::next_matching(
            &mut events,
            |event| matches!(event, GossipsubEvent::Subscribed { peer_id: id } if *id == peer_id),
        )
        .await;
        assert_eq!(
            node.gossipsub().peers("topic").await.unwrap(),
            vec![peer_id]
        );

        peer.gossipsub().unsubscribe("topic").await.unwrap();
        common::next_matching(
            &mut events,
            |event| matches!(event, GossipsubEvent::Unsubscribed { peer_id: id } if *id == peer_id),
        )
        .await;
        assert!(node.gossipsub().peers("topic").await.unwrap().is_empty());
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn invalid_subscription_changes_return_errors() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_gossipsub()).await;
        node.gossipsub().subscribe("topic").await.unwrap();
        assert!(matches!(
            node.gossipsub().subscribe("topic").await,
            Err(Error::Gossipsub(gossipsub::Error::AlreadySubscribed(topic))) if topic.as_str() == "topic"
        ));
        node.gossipsub().unsubscribe("topic").await.unwrap();
        assert!(matches!(
            node.gossipsub().unsubscribe("topic").await,
            Err(Error::Gossipsub(gossipsub::Error::NotSubscribed(topic))) if topic.as_str() == "topic"
        ));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_gossipsub_returns_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        for result in [
            node.gossipsub().subscribe("topic").await,
            node.gossipsub().unsubscribe("topic").await,
            node.gossipsub().listener("topic").await.map(|_| ()),
            node.gossipsub().peers("topic").await.map(|_| ()),
            node.gossipsub().publish("topic", "message").await,
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Gossipsub
                })
            ));
        }
    })
    .await;
}
