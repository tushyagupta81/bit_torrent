pub mod central_manager;
pub mod events;
pub mod files;
pub mod network;
pub mod peers;
pub mod peers_task;
pub mod tracker;

use std::error::Error;
use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinSet};

use crate::engine::central_manager::PieceCommands;
use crate::engine::{
    central_manager::CentralManager, events::UiEvent, peers_task::Peer, tracker::fetch_peers,
};

use crate::bencode::MetaInfo;
type AsyncError = Box<dyn Error + Send + Sync>;

pub async fn spawn_engine(
    info: Arc<MetaInfo>,
    ui_tx: mpsc::Sender<UiEvent>,
) -> Result<(), AsyncError> {
    files::initialize_files(&info.info)?;

    let peers = fetch_peers(&info).await?;
    let piece_count = info.info.pieces.len().div_ceil(20);

    let (cmd_tx, cmd_rx) = mpsc::channel(256);

    let central = CentralManager::new(piece_count, ui_tx.clone());

    let mut join_set = JoinSet::new();

    join_set.spawn(async move {
        central.run(cmd_rx).await;
    });

    for peer_addr in peers.peers.0 {
        let torrent = info.clone();
        let ui_tx = ui_tx.clone();
        let cmd_tx = cmd_tx.clone();

        join_set.spawn(async move {
            if let Ok(mut peer) = Peer::new(peer_addr, torrent, cmd_tx, ui_tx).await {
                match peer.start().await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error {e}");
                    }
                }
                let _ = peer
                    .sender
                    .send(PieceCommands::PeerDead(peer.peer_id))
                    .await;
                let _ = peer
                    .ui_tx
                    .send(UiEvent::PeerDisconnected(
                        String::from_utf8_lossy(&peer.peer_id).to_string(),
                    ))
                    .await.ok();
            }
        });
    }

    // Detach engine tasks (UI controls lifetime)
    tokio::spawn(async move {
        while let Some(res) = join_set.join_next().await {
            let _ = res;
        }
    });

    Ok(())
}
