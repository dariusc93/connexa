# Distributed Key-Value Store Example

This example demonstrates how to build a distributed key-value store using Kademlia DHT implementation.

## Overview

The example creates a peer-to-peer network where nodes can:

- Store and retrieve key-value pairs across the network
- Provide and discover content providers
- Use Kademlia DHT for distributed storage and routing

## Running the Example

### Single Node

```bash
cargo run
```

### Multiple Nodes

Start the first node:

```bash
cargo run
```

Start additional nodes and connect to the first:

```bash
cargo run -- --peer <MULTIADDR_OF_FIRST_NODE>
```

### With Record Filtering

Enable filtering of incoming records:

```bash
cargo run -- --enable-filtering
```