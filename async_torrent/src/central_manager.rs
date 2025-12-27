use std::collections::{HashMap};

use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Debug)]
struct PeerState {
    choked: bool,
    interested: bool,
    bitfield: Vec<bool>,
    outstanding: usize,
}

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum PieceState {
    Free,
    Done,
    Reserved(PeerId),
}

#[derive(Debug)]
pub struct CentralManager {
    peers: HashMap<PeerId, PeerState>,
    pieces_status: Vec<PieceState>,
}

pub type PeerId = [u8; 20];

pub enum PieceCommands {
    RequestPieceIndex(PeerId, oneshot::Sender<Option<usize>>),
    PieceDone(PeerId, usize),
    PieceFailed(PeerId, usize),
    UpdateBitfield(PeerId, Vec<bool>),
}

impl CentralManager {
    async fn run(mut self, mut rx: mpsc::Receiver<PieceCommands>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                PieceCommands::RequestPieceIndex(peer_id, sender) => {
                    // Get a piece index for the peer
                    if let Some(peer_info) = self.peers.get(&peer_id) {
                        let index = self
                            .pieces_status
                            .iter()
                            .enumerate()
                            .find(|(i, state)| {
                                **state == PieceState::Free && peer_info.bitfield[*i]
                            })
                            .map(|(i, _)| {
                                self.pieces_status[i] == PieceState::Reserved(peer_id);
                                i
                            });
                        let _ = sender.send(index);
                    } else {
                        sender.send(None);
                    }
                }
                PieceCommands::PieceDone(_peer_id, piece_index) => {
                    self.pieces_status[piece_index] = PieceState::Done;
                }
                PieceCommands::PieceFailed(_peer_id, piece_index) => {
                    self.pieces_status[piece_index] = PieceState::Free;
                }
                PieceCommands::UpdateBitfield(peer_id, new_bitfield) => {
                    if let Some(peer_info) = self.peers.get_mut(&peer_id) {
                        peer_info.bitfield = new_bitfield;
                    }
                }
            }
        }
    }
}
