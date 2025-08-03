# Custom Transport Example

This example demonstrates how to use a custom transport with Connexa instead of the default TCP/QUIC transports.

## Running the Example

```bash
cargo run
```

## Use Cases

Custom transports are useful for:

- Implementing transport protocols, overriding the internal transport
- Creating transport adapters for specific environments