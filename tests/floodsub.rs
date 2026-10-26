#![cfg(feature = "floodsub")]

mod common;

use connexa::error::{Error, Protocol, floodsub};
use connexa::prelude::{FloodsubEvent, FloodsubMessage, PeerId};
use futures::stream::BoxStream;

async fn next_message(events: &mut BoxStream<'static, FloodsubEvent>) -> FloodsubMessage {
    let event = common::next_matching(events, |event| {
        matches!(event, FloodsubEvent::Message { .. })
    })
    .await;
    let FloodsubEvent::Message { message } = event else {
        unreachable!();
    };
    message
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn deliver_messages_to_topic_listeners() {
    common::timeout(async {
        let (sender, sender_id, _) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let (receiver, receiver_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let mut first_subscriptions = sender.floodsub().listener("first").await.unwrap();
        let mut second_subscriptions = sender.floodsub().listener("second").await.unwrap();
        let mut first_events = receiver.floodsub().listener("first").await.unwrap();
        let mut another_listener = receiver.floodsub().listener("first").await.unwrap();
        let mut second_events = receiver.floodsub().listener("second").await.unwrap();

        sender.swarm().dial(addr).await.unwrap();
        sender
            .floodsub()
            .add_node_to_partial_view(receiver_id)
            .await
            .unwrap();
        receiver
            .floodsub()
            .add_node_to_partial_view(sender_id)
            .await
            .unwrap();
        for topic in ["first", "second"] {
            sender.floodsub().subscribe(topic).await.unwrap();
            receiver.floodsub().subscribe(topic).await.unwrap();
        }
        for events in [&mut first_subscriptions, &mut second_subscriptions] {
            common::next_matching(events, |event| {
                matches!(event, FloodsubEvent::Subscribed { peer_id } if *peer_id == receiver_id)
            })
            .await;
        }

        sender
            .floodsub()
            .publish("first", "first message")
            .await
            .unwrap();
        sender
            .floodsub()
            .publish("second", "second message")
            .await
            .unwrap();
        let first = next_message(&mut first_events).await;
        assert_eq!(first.source, sender_id);
        assert_eq!(first.data.as_ref(), b"first message");
        assert_eq!(next_message(&mut another_listener).await, first);
        assert_eq!(
            next_message(&mut second_events).await.data.as_ref(),
            b"second message"
        );

        drop(another_listener);
        sender
            .floodsub()
            .publish_many(["first", "second"], "shared message")
            .await
            .unwrap();
        let shared = next_message(&mut first_events).await;
        assert_eq!(shared.source, sender_id);
        assert_eq!(shared.data.as_ref(), b"shared message");
        assert_eq!(next_message(&mut second_events).await, shared);
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn publish_without_a_local_subscription() {
    common::timeout(async {
        let (sender, sender_id, _) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let (receiver, receiver_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let mut first_subscriptions = sender.floodsub().listener("first").await.unwrap();
        let mut second_subscriptions = sender.floodsub().listener("second").await.unwrap();
        let mut first_events = receiver.floodsub().listener("first").await.unwrap();
        let mut second_events = receiver.floodsub().listener("second").await.unwrap();

        sender.swarm().dial(addr).await.unwrap();
        sender
            .floodsub()
            .add_node_to_partial_view(receiver_id)
            .await
            .unwrap();
        receiver.floodsub().subscribe("first").await.unwrap();
        receiver.floodsub().subscribe("second").await.unwrap();
        for events in [&mut first_subscriptions, &mut second_subscriptions] {
            common::next_matching(events, |event| {
                matches!(event, FloodsubEvent::Subscribed { peer_id } if *peer_id == receiver_id)
            })
            .await;
        }

        sender
            .floodsub()
            .publish_any("first", "single message")
            .await
            .unwrap();
        let message = next_message(&mut first_events).await;
        assert_eq!(message.source, sender_id);
        assert_eq!(message.data.as_ref(), b"single message");

        sender
            .floodsub()
            .publish_many_any(["first", "second"], "shared message")
            .await
            .unwrap();
        let shared = next_message(&mut first_events).await;
        assert_eq!(shared.data.as_ref(), b"shared message");
        assert_eq!(next_message(&mut second_events).await, shared);
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn track_remote_subscriptions() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let (peer, peer_id, addr) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        let mut events = node.floodsub().listener("topic").await.unwrap();
        node.swarm().dial(addr).await.unwrap();

        peer.floodsub().subscribe("topic").await.unwrap();
        common::next_matching(
            &mut events,
            |event| matches!(event, FloodsubEvent::Subscribed { peer_id: id } if *id == peer_id),
        )
        .await;
        peer.floodsub().unsubscribe("topic").await.unwrap();
        common::next_matching(
            &mut events,
            |event| matches!(event, FloodsubEvent::Unsubscribed { peer_id: id } if *id == peer_id),
        )
        .await;
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn invalid_subscription_changes_return_errors() {
    common::timeout(async {
        let (node, _, _) =
            common::spawn_connexa_with_config(|builder| builder.with_floodsub()).await;
        node.floodsub().subscribe("topic").await.unwrap();
        assert!(matches!(
            node.floodsub().subscribe("topic").await,
            Err(Error::Floodsub(floodsub::Error::AlreadySubscribed(topic))) if topic.id() == "topic"
        ));
        node.floodsub().unsubscribe("topic").await.unwrap();
        assert!(matches!(
            node.floodsub().unsubscribe("topic").await,
            Err(Error::Floodsub(floodsub::Error::NotSubscribed(topic))) if topic.id() == "topic"
        ));
    })
    .await;
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn disabled_floodsub_returns_errors() {
    common::timeout(async {
        let (node, _, _) = common::spawn_connexa_with_config(|builder| builder).await;
        let peer_id = PeerId::random();
        for result in [
            node.floodsub().subscribe("topic").await,
            node.floodsub().unsubscribe("topic").await,
            node.floodsub().listener("topic").await.map(|_| ()),
            node.floodsub().add_node_to_partial_view(peer_id).await,
            node.floodsub().remove_node_from_partial_view(peer_id).await,
            node.floodsub().publish("topic", "message").await,
            node.floodsub().publish_any("topic", "message").await,
            node.floodsub().publish_many(["topic"], "message").await,
            node.floodsub().publish_many_any(["topic"], "message").await,
        ] {
            assert!(matches!(
                result,
                Err(Error::Disabled {
                    protocol: Protocol::Floodsub
                })
            ));
        }
    })
    .await;
}
