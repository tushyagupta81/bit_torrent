use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use tokio::sync::mpsc;

use crate::engine::events::UiEvent;

#[derive(Clone, Copy)]
pub enum PieceState {
    Missing,
    Requested,
    Downloading,
    Complete,
}

pub struct PeerStatus {
    pub peer_id: String,
    pub task: Vec<String>,
    pub choked: bool,
}

pub struct AppState {
    pub pieces: Vec<PieceState>,
    pub peers: VecDeque<PeerStatus>,
}

impl AppState {
    pub fn new(num_pieces: usize) -> Self {
        AppState {
            pieces: vec![PieceState::Missing; num_pieces],
            peers: VecDeque::new(),
        }
    }
}
pub async fn process_event(app_state: Arc<RwLock<AppState>>, ui_rx: &mut mpsc::Receiver<UiEvent>) {
    while let Some(event) = ui_rx.recv().await {
        {
            let mut state = app_state.write().unwrap();
            match event {
                UiEvent::PieceRequested(index) => {
                    state.pieces[index] = PieceState::Requested;
                }
                UiEvent::PieceDownloading(index) => {
                    state.pieces[index] = PieceState::Downloading;
                }
                UiEvent::PieceCompleted(index) => {
                    state.pieces[index] = PieceState::Complete;
                }
                UiEvent::PeerUpdate {
                    peer_id,
                    task,
                    choked,
                } => {
                    if let Some(peer) = state.peers.iter_mut().find(|p| p.peer_id == peer_id) {
                        peer.task.push(task);
                        peer.choked = choked;
                    } else {
                        state.peers.push_back(PeerStatus {
                            peer_id,
                            task: vec![task],
                            choked,
                        });
                    }
                }
                UiEvent::PeerDisconnected(peer_id) => {
                    state.peers.retain(|p| p.peer_id != peer_id);
                }
            }
        }
    }
}
