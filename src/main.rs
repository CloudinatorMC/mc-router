//! Very small experimental Minecraft handshake-routing TCP proxy.
//! It inspects the initial Minecraft handshake to extract the 'server address'
//! (the string the client typed into multiplayer) and uses simple prefix / exact
//! matching rules to forward the TCP stream to a configured backend server.
//!
//! Supports only modern (VarInt-based) handshake up to the point of deciding
//! which backend to connect; afterwards it just pipes bytes in both directions.
//! No encryption / compression handling is performed (act as a plain TCP hop).

use anyhow::{anyhow, Result, Context};
use std::{collections::HashMap, net::{IpAddr, SocketAddr}, sync::Arc};
use tokio::{io::AsyncWriteExt, net::{TcpListener, TcpStream}, sync::RwLock};
use tracing::{info, warn};

mod config;
mod protocol;
mod routing;
mod udp;

use config::Config;
use protocol::{parse_handshake_server_address, read_framed_packet};
use routing::{route_backend, sanitize_address};
use udp::spawn_udp_forwarder;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cfg = Arc::new(RwLock::new(Config::load(None)?));
    let listen_addr = cfg.read().await.listen.clone();
    let udp_client_map: Arc<RwLock<HashMap<IpAddr, String>>> = Arc::new(RwLock::new(HashMap::new()));

    // Spawn UDP forwarding task (best-effort).
    if let Err(e) = spawn_udp_forwarder(&listen_addr, cfg.clone(), udp_client_map.clone()).await {
        warn!(error=%e, "failed starting UDP forwarder");
    }
    let listener = TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("binding {listen_addr}"))?;
    info!(%listen_addr, "listening");

    loop {
        let (socket, addr) = listener.accept().await?;
        let cfg = cfg.clone();
        let udp_client_map = udp_client_map.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, addr, cfg, udp_client_map).await {
                warn!(client=%addr, error=%e, "connection error");
            }
        });
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,mc_router=debug".into()),
        )
        .try_init();
}

async fn handle_client(
    mut inbound: TcpStream,
    peer: SocketAddr,
    cfg: Arc<RwLock<Config>>,
    udp_client_map: Arc<RwLock<HashMap<IpAddr, String>>>,
) -> Result<()> {
    // Read exactly one Minecraft packet (length VarInt + payload) for the initial handshake.
    let buf = read_framed_packet(&mut inbound).await?;

    let server_addr_raw = parse_handshake_server_address(&buf)?;
    let server_addr_lc = sanitize_address(&server_addr_raw);
    if server_addr_lc != server_addr_raw.to_ascii_lowercase() {
        info!(original=%server_addr_raw, sanitized=%server_addr_lc, "sanitized server address");
    }
    let cfg_guard = cfg.read().await;
    let backend = route_backend(&server_addr_lc, &cfg_guard)
        .ok_or_else(|| anyhow!("no route for {server_addr_raw}"))?;
    info!(client=%peer, requested=%server_addr_raw, backend=%backend, "routing");

    // Record mapping from client IP -> backend for later UDP forwarding (best-effort).
    {
        let mut map = udp_client_map.write().await;
        map.insert(peer.ip(), backend.clone());
    }

    let mut outbound = TcpStream::connect(&backend).await?;
    // Write the handshake we consumed to outbound first, preserving transparency.
    outbound.write_all(&buf).await?;

    // Now pipe both directions.
    let (mut ri, mut wi) = inbound.into_split();
    let (mut ro, mut wo) = outbound.into_split();

    let c2s = tokio::spawn(async move { tokio::io::copy(&mut ri, &mut wo).await });
    let s2c = tokio::spawn(async move { tokio::io::copy(&mut ro, &mut wi).await });

    let _ = tokio::try_join!(c2s, s2c);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routing::{route_backend, sanitize_address}, protocol::read_varint, config::Config};

    #[test]
    fn test_varint_roundtrip() {
        let data = [0xAC, 0x02]; // 300
        let (v, idx) = read_varint(&data, 0).unwrap();
        assert_eq!(v, 300);
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_route_default_used() {
        let cfg = Config { listen: "0.0.0.0:0".into(), routes: HashMap::new(), default: Some("127.0.0.1:12345".into()) };
        let b = route_backend("unknown.example", &cfg);
        assert_eq!(b.as_deref(), Some("127.0.0.1:12345"));
    }

    #[test]
    fn test_sanitize_address() {
        assert_eq!(sanitize_address("Example.COM."), "example.com");
        assert_eq!(sanitize_address("play.example.com\0FML2"), "play.example.com");
    }
}
