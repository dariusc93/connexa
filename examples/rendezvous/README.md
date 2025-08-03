# Rendezvous Example

This example demonstrates how to use Connexa with the rendezvous protocol for decentralized peer discovery and
registration.

## Overview

The rendezvous protocol allows peers to:

- Register themselves under namespaces at rendezvous points
- Discover other peers registered under specific namespaces
- Build decentralized discovery mechanisms without centralized trackers

## Usage

### 1. Start a Rendezvous Server

```bash
cargo run -- --mode server --seed 1
```

### 2. Start Client Nodes

Start first client:

```bash
cargo run -- --mode client --seed 2 --rendezvous-node /ip4/127.0.0.1/tcp/62649/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN
```

Start second client:

```bash
cargo run -- --mode client --seed 3 --rendezvous-node /ip4/127.0.0.1/tcp/62649/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN
```

## Interactive Commands (Client Mode)

### Register with a namespace

```
register <RENDEZVOUS_PEER_ID> <NAMESPACE>
```

### Discover peers in a namespace

```
discover <RENDEZVOUS_PEER_ID> [NAMESPACE]
```

### Unregister from a namespace

```
unregister <RENDEZVOUS_PEER_ID> <NAMESPACE>
```

## Use Cases

The rendezvous protocol is ideal for:

- Topic-based pubsub systems
- Service discovery in P2P networks
- Bootstrapping P2P applications