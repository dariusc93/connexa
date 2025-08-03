# Gossipsub Example

This example demonstrates how to use Connexa with the Gossipsub protocol for publish-subscribe messaging in a
peer-to-peer network.

## Usage

### Single Node (Create a Topic)

```bash
cargo run -- --topic my-chat-room
```

### Join Existing Network

```bash
cargo run -- --topic my-chat-room --peers <MULTIADDR>
```

### Custom Listening Address

```bash
cargo run -- --topic my-chat-room --listener /ip4/0.0.0.0/tcp/9000
```

## Example Workflow

1. **Terminal 1** - Start first node:

```bash
cargo run -- --topic gossip-demo
```

2. **Terminal 2** - Join the network:

```bash
cargo run -- --topic gossip-demo --peers /ip4/127.0.0.1/tcp/54321/p2p/12D3KooW...
```

3. **Send Messages**: Type messages in either terminal and press Enter. Messages will be broadcast to all peers
   subscribed to the topic.

## Interactive Commands

While running, you can:

- Type any message and press Enter to broadcast it
- Press Ctrl+C to exit
