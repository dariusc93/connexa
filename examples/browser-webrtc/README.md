# Browser WebRTC Example

This example demonstrates how to use WebRTC transport to enable peer-to-peer communication between a native node and a
web browser.

## Overview

The example consists of two parts:

- **Server (Native)**: A node that runs a WebRTC peer and serves a web interface
- **Client (WASM)**: A WebAssembly module that runs in the browser and connects to the server

## Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` target
- `wasm-pack` for building the WASM module

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Add WASM target
rustup target add wasm32-unknown-unknown
```

## Building

First, build the WASM module:

```bash
# From this directory
wasm-pack build --target web --out-dir dist
```

## Running

1. Start the server:

```bash
cargo run
```

The server will:

- Start a WebRTC listener on a random UDP port
- Serve the web interface on `http://<IP>:8080`
- Display the multiaddr it's listening on

2. Open your browser and navigate to the URL shown in the console

3. The browser will automatically connect to the server using WebRTC