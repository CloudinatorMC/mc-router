use tracing::*;
use wynd::wynd::Wynd;
use tokio::task::JoinHandle;
use std::sync::Arc;
use tokio::sync::RwLock;

pub fn start_management_api(port: u16, _state: Arc<RwLock<super::AppState>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut wynd = Wynd::new();

        wynd.on_connection(|conn| {
            conn.on_text(|event| async move {
                println!("Received message: {}", event.data);
            });
        });

        if let Err(e) = wynd.listen(port, || {
            info!("Listening on port {port}");
        }).await
        {
            error!("Wynd server failed: {}", e);
        }
    })
}
