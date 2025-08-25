# mc-router

Minimal experimental Minecraft handshake routing TCP proxy written in Rust.

It inspects the initial handshake packet and reads the "server address" field the client provided in the multiplayer server list. That textual value (lowercased) is matched against configured route keys to decide which backend server to forward the connection to. The handshake bytes are passed through unchanged; afterwards the proxy just pipes bytes in both directions.

## Features

- VarInt-based parsing of the first handshake packet only
- Exact and prefix route matching (longest prefix wins)
- Optional default backend
- Simple TOML configuration file (`mc-router.toml`)
- Async I/O via Tokio
- Structured logging via `tracing`
- UDP packet forwarding for mods that may use it

## Getting Started

1. Copy or edit `mc-router.toml` to define your routes.
2. Build and run:

```
cargo run --release
```

3. Point your Minecraft client at one of the configured route addresses (e.g. `lobby.example.com`). The proxy will forward to the mapped backend.

### Configuration File

```
listen = "0.0.0.0:25565"
# default = "127.0.0.1:25569"  # optional fallback

[routes]
"lobby.example.com" = "127.0.0.1:25566"
"survival.example.com" = "127.0.0.1:25567"
"mini-" = "127.0.0.1:25568"  # prefix example

```

Route selection order:
1. Exact match of the full address string
2. Longest prefix match (a key that is a prefix of the address)
3. `default` backend if specified

If no rule matches, the connection is closed and a warning is logged.

## License

GPL-3.0
