# UPnP Example

This example demonstrates how to use Connexa with UPnP (Universal Plug and Play) for automatic port forwarding on
routers that support it.

## Overview

UPnP allows applications to automatically:

- Discover network gateways (routers)
- Request port forwarding mappings
- Obtain external IP addresses
- Enable connectivity from the internet without manual router configuration

## Running the Example

```bash
cargo run
```

## Notes

If UPnP is not available, you can:

1. Manually configure port forwarding on your router
2. Use a relay service (see relay example)