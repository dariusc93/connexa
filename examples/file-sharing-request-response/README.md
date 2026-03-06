# File Sharing Example

This example demonstrates how to build a simple peer-to-peer file sharing application using the request-response
protocol.

## Overview

The example creates a file sharing network where:

- Nodes can provide files to the network
- Other nodes can request and download files by name
- Uses libp2p's request-response protocol for file transfers

## Usage

### Providing a File

To share a file on the network:

```bash
cargo run -- provide --path /path/to/file.txt --name myfile
```

### Getting a File

To download a file from a provider:

```bash
cargo run -- --peer <PROVIDER_ADDRESS> get --name myfile
```