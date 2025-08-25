use crate::config::Config;
use crate::routing::route_backend; // might be useful later
use anyhow::Result;
use std::{collections::HashMap, net::IpAddr, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, sync::RwLock};
use tracing::{info, warn};

pub async fn spawn_udp_forwarder(
    listen_addr: &str,
    cfg: Arc<RwLock<Config>>,
    udp_client_map: Arc<RwLock<HashMap<IpAddr, String>>>,
) -> Result<()> {
    let sock = UdpSocket::bind(listen_addr).await?; // Bind same addr/port for UDP.
    let sock = Arc::new(sock);
    info!(addr=%listen_addr, "UDP listener active");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65_535];
        loop {
            let (len, src) = match sock.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(error=%e, "udp recv error");
                    continue;
                }
            };
            let backend_addr_opt = if let Some(b) = udp_client_map.read().await.get(&src.ip()).cloned() {
                Some(b)
            } else {
                let g = cfg.read().await;
                g.default.as_ref().map(|d| d.address.clone())
            };
            let Some(backend) = backend_addr_opt else { continue }; // Drop if no mapping/default.
            let data = buf[..len].to_vec();
            let sock_clone = sock.clone();
            tokio::spawn(async move {
                if let Err(e) = forward_single_udp(&backend, &data, sock_clone, src).await {
                    warn!(client=%src, backend=%backend, error=%e, "udp forward error");
                }
            });
        }
    });
    Ok(())
}

async fn forward_single_udp(
    backend: &str,
    data: &[u8],
    listen_sock: Arc<UdpSocket>,
    client_addr: SocketAddr,
) -> Result<()> {
    // Create ephemeral socket for this exchange. Keeps implementation simple.
    let upstream = UdpSocket::bind("0.0.0.0:0").await?;
    upstream.connect(backend).await?;
    upstream.send(data).await?;
    // Attempt a single response with short timeout.
    let mut resp = vec![0u8; 65_535];
    match tokio::time::timeout(Duration::from_secs(2), upstream.recv(&mut resp)).await {
        Ok(Ok(n)) => {
            let _ = listen_sock.send_to(&resp[..n], client_addr).await;
        }
        _ => { /* ignore timeouts/errors */ }
    }
    Ok(())
}
