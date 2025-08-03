# Custom Behaviour and Context Example

This example demonstrates how to integrate custom libp2p behaviours with Connexa, along with custom context and command
handling.

## Overview

The example shows:

- Creating a custom NetworkBehaviour implementation
- Using a custom context to manage state
- Implementing a command pattern for behaviour interaction
- Handling custom events from the behaviour

## Running the Example

```bash
cargo run
```

## Use Cases

This pattern is useful for:

- Implementing custom protocols not provided by libp2p
- Managing application state that needs to interact with the network layer
- Building complex state machines that respond to network events
- Creating request/response patterns over custom protocols