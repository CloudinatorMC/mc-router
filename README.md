# mc-router

Minimal experimental Minecraft handshake routing TCP proxy written in Rust.

It inspects the initial handshake packet and reads the "server address" field the client provided in the multiplayer server list. That textual value (lowercased) is matched against configured route keys to decide which backend server to forward the connection to. The handshake bytes are passed through unchanged; afterwards the proxy just pipes bytes in both directions.

## Features

- VarInt-based parsing of the first handshake packet only
- Exact and prefix route matching (longest prefix wins)
- Optional default backend
- Per-backend options (e.g. HAProxy PROXY protocol injection)
- Simple TOML configuration file (`mc-router.toml`)
- Async I/O via Tokio
- Structured logging via `tracing`
- UDP packet forwarding for mods that may use it
- Config synchronisation via Control Plane API (optional, see `control_plane` module)

## Getting Started

1. Copy or edit `mc-router.toml` to define your routes.
2. Build and run:

```
cargo run --release
```

3. Point your Minecraft client at one of the configured route addresses (e.g. `lobby.example.com`). The proxy will forward to the mapped backend.

### Configuration File

A backend can be a simple string (address) or a table with options.

```
listen = "0.0.0.0:25565"

# default can be simple or detailed
# default = "127.0.0.1:25569"
default = { address = "127.0.0.1:25566", useHAProxy = true }

[routes]
"lobby.example.com"    = "127.0.0.1:25566"                  # simple form
"survival.example.com" = { address = "127.0.0.1:25567", useHAProxy = false }
"mini-"                = { address = "127.0.0.1:25568" }     # prefix form
```

#### Field: `useHAProxy`

When `true`, the proxy injects an [HAProxy PROXY protocol v1](https://www.haproxy.org/download/1.8/doc/proxy-protocol.txt) header line *before* forwarding the original Minecraft handshake bytes to the backend. Example header (IPv4):

```
PROXY TCP4 203.0.113.5 10.0.0.2 54321 25565\r\n
```

This conveys the real client IP/port so the backend (or another downstream proxy like Velocity/Bungee) can apply IP-based logic and show the correct player address. Only enable this if the backend is configured to expect and trust PROXY protocol; otherwise it will interpret the header as part of the Minecraft packet and fail the connection.

Notes:
* Automatically chooses `TCP4` vs `TCP6`; if address families differ it falls back to `PROXY UNKNOWN`.
* Currently only PROXY v1 is implemented (human-readable). Adding v2 (binary) would be straightforward if needed.
* The header precedes the exact handshake bytes the client sent (transparency maintained after injection).

Route selection order:
1. Exact match of the full address string
2. Longest prefix match (a key that is a prefix of the address)
3. `default` backend if specified

If no rule matches, the connection is closed and a warning is logged.

## License

GPL-3.0
