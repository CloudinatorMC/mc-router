//! CloudinatorMC - mc-router
//! Very small experimental Minecraft handshake-routing TCP proxy.
//! It inspects the initial Minecraft handshake to extract the 'server address'
//! (the string the client typed into multiplayer) and uses simple prefix / exact
//! matching rules to forward the TCP stream to a configured backend server.
//!
//! Supports only modern (VarInt-based) handshake up to the point of deciding
//! which backend to connect; afterwards it just pipes bytes in both directions.

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
use std::env;
use ed25519_dalek::PublicKey;
use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine as _;
// Use the shared config-service library for config service functionality.
use config_service_lib::ConfigServiceClient;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cfg = Arc::new(RwLock::new(Config::load(None)?));
    // Optionally start control-plane poller if env vars provided
    if let (Ok(base), Ok(rid), Ok(pubkey_b64)) = (env::var("CONFIG_SERVICE_URL"), env::var("OBJECT_ID"), env::var("CONFIG_SERVICE_PUBKEY")) {
        if let Ok(pk_bytes) = BASE64_ENGINE.decode(pubkey_b64.as_bytes()) {
            if let Ok(pubkey) = PublicKey::from_bytes(&pk_bytes) {
                // Construct shared ConfigServiceClient from the library and spawn its poll loop.
                let cp = ConfigServiceClient::new(base.clone(), rid.clone(), pubkey);
                // Start poll loop in background
                let cp_for_poll = cp.clone();
                tokio::spawn(async move {
                    if let Err(e) = cp_for_poll.poll_loop().await {
                        tracing::error!(error=%e, "control plane poller failed");
                    }
                });
                // Start a small task that periodically snapshots routes and merges into runtime cfg
                let cp_for_merge = cp.clone();
                let cfg_for_merge = cfg.clone();
                tokio::spawn(async move {
                    loop {
                        let assigns = cp_for_merge.snapshot_assignments().await;
                        // Build a fresh routes map from all assignment blobs, then
                        // replace the runtime routes atomically. This ensures routes
                        // removed from the assignments are removed from the router.
                        let mut new_routes: HashMap<String, crate::config::BackendConfig> = HashMap::new();
                        for (_tenant, a) in assigns.iter() {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&a.blob) {
                                if let Some(routes) = v.get("routes").and_then(|r| r.as_object()) {
                                    for (k, v) in routes {
                                        if let Some(s) = v.as_str() {
                                            new_routes.insert(k.to_ascii_lowercase(), crate::config::BackendConfig { address: s.to_string(), use_haproxy: false });
                                        } else if let Some(obj) = v.as_object() {
                                            if let Some(addr) = obj.get("address").and_then(|x| x.as_str()) {
                                                let use_h = obj.get("use_haproxy").and_then(|x| x.as_bool()).unwrap_or(false);
                                                new_routes.insert(k.to_ascii_lowercase(), crate::config::BackendConfig { address: addr.to_string(), use_haproxy: use_h });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let mut cfg_w = cfg_for_merge.write().await;
                        cfg_w.routes = new_routes;
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                });
            } else { warn!("invalid CONFIG_SERVICE_PUBKEY bytes"); }
        } else { warn!("failed decoding CONFIG_SERVICE_PUBKEY base64"); }
    }
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
    let backend_cfg = route_backend(&server_addr_lc, &cfg_guard)
        .ok_or_else(|| anyhow!("no route for {server_addr_raw}"))?;
    let backend = &backend_cfg.address;
    info!(client=%peer, requested=%server_addr_raw, backend=%backend, haproxy=%backend_cfg.use_haproxy, "routing");

    // Record mapping from client IP -> backend for later UDP forwarding (best-effort).
    {
        let mut map = udp_client_map.write().await;
        map.insert(peer.ip(), backend.clone());
    }

    let mut outbound = TcpStream::connect(&backend).await?;
    // If HAProxy PROXY protocol v1 is enabled, send header first.
    if backend_cfg.use_haproxy {
        // Determine address family and format accordingly (only TCP4/TCP6 supported here).
        let client_ip = peer.ip();
        let local_ip = outbound.local_addr()?.ip();
        let fam = match (client_ip, local_ip) {
            (std::net::IpAddr::V4(_), std::net::IpAddr::V4(_)) => "TCP4",
            (std::net::IpAddr::V6(_), std::net::IpAddr::V6(_)) => "TCP6",
            _ => "UNKNOWN", // mixed family - spec permits using UNKNOWN which omits addresses
        };
        let header = if fam == "UNKNOWN" {
            format!("PROXY UNKNOWN\r\n")
        } else {
            format!(
                "PROXY {fam} {} {} {} {}\r\n",
                client_ip,
                local_ip,
                peer.port(),
                outbound.local_addr()?.port()
            )
        };
        outbound.write_all(header.as_bytes()).await?;
    }
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
        let cfg = Config { listen: "0.0.0.0:0".into(), routes: HashMap::new(), default: Some(config::BackendConfig { address: "127.0.0.1:12345".into(), use_haproxy: false }) };
        let b = route_backend("unknown.example", &cfg).map(|b| &b.address);
        assert_eq!(b, Some(&"127.0.0.1:12345".to_string()));
    }

    #[test]
    fn test_sanitize_address() {
        assert_eq!(sanitize_address("Example.COM."), "example.com");
        assert_eq!(sanitize_address("play.example.com\0FML2"), "play.example.com");
    }
}
