use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tracing::*;
use tokio::task::JoinHandle;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};

mod rpc;
mod routes;

use crate::management_api::routes::handle_request;

pub fn start_management_api(addr: String, state: Arc<RwLock<super::AppState>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&addr).await.expect("Can't listen");
        info!("Management API Listening on: {}", addr);

        while let Ok((stream, _)) = listener.accept().await {
            let peer = stream.peer_addr().expect("connected streams should have a peer address");
            info!("Management API peer connection, address: {}", peer);

            tokio::spawn(accept_connection(peer, stream, state.clone()));
        }
    })
}

async fn accept_connection(peer: SocketAddr, stream: TcpStream, state: Arc<RwLock<super::AppState>>) {
    if let Err(e) = handle_connection(peer, stream, state).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8(_) => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream, state: Arc<RwLock<super::AppState>>) -> Result<()> {
    let mut ws_stream = accept_async(stream).await.expect("Failed to accept");

    info!("New WebSocket connection: {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        if msg.is_text() {
            if let Some(text) = msg.into_text().ok() {
                let state = state.clone();
                let response = handle_request(&text, state).await;
                
                if let Ok(resp_json) = serde_json::to_string(&response) {
                    ws_stream.send(Message::Text(resp_json.into())).await?;
                } else {
                    error!("Failed to serialize response");
                }
            }
        }
    }

    Ok(())
}

