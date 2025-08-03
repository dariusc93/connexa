# Relay Example

This example demonstrates how to use Connexa with libp2p's relay protocol to enable connectivity between peers that
cannot directly connect to each other (e.g., behind NATs or firewalls).

## Overview

The relay protocol allows peers to communicate through an intermediary relay server. This example shows three roles:

- **Server**: Acts as a relay server that forwards traffic between peers
- **Listen**: A peer that listens for connections through a relay
- **Dial**: A peer that connects to another peer through a relay

## Usage

### 1. Start a Relay Server

```bash
cargo run -- --mode server --seed 1
```

### 2. Start a Listening Peer

```bash
cargo run -- --mode listen --seed 2 --relay-address /ip4/127.0.0.1/tcp/12345/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN
```

### 3. Dial the Listening Peer

```bash
cargo run -- --mode dial --seed 3 --relay-address /ip4/127.0.0.1/tcp/12345/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN --remote-peer-id 12D3KooWH3uVF6wv47WnArKHk5ZDVzgNFsge1BpzMNDKHfQjRQkY
```

## How It Works

1. **Relay Server**:
    - Enables relay server functionality
    - Advertises circuit relay addresses
    - Accepts reservation requests from clients

2. **Listening Peer**:
    - Connects to the relay server
    - Listens on a circuit relay address
    - Can receive connections through the relay

3. **Dialing Peer**:
    - Connects to the relay server
    - Dials the listening peer through the circuit relay
    - Establishes end-to-end encrypted connection

## Use Cases

This pattern is essential for:

- Connecting peers behind NATs or firewalls
- Mobile applications with changing network conditions
- IoT devices with restricted connectivity
- Building resilient P2P networks