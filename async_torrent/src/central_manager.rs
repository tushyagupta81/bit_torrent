use std::collections::HashMap;

use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Debug)]
struct PeerState {
    choked: bool,
    interested: bool,
    bitfield: Vec<bool>,
    outstanding: usize,
}

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone)]
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
    UpdateBitfield(PeerId, u32),
    PeerChoked(PeerId),
    SetBitfield(PeerId, Vec<bool>),
    PeerUnchoke(PeerId),
    PeerRegister(PeerId, usize),
}

impl CentralManager {
    pub fn new(num_piece: usize) -> CentralManager {
        CentralManager {
            peers: HashMap::new(),
            pieces_status: vec![PieceState::Free; num_piece],
        }
    }
    pub async fn run(mut self, mut rx: mpsc::Receiver<PieceCommands>) {
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
                            .map(|(i, _)| i);
                        if let Some(i) = index {
                            self.pieces_status[i] = PieceState::Reserved(peer_id);
                        }
                        let _ = sender.send(index);
                    } else {
                        let _ = sender.send(None);
                    }
                }
                PieceCommands::PieceDone(_peer_id, piece_index) => {
                    // println!("Piece {} done", piece_index);
                    self.pieces_status[piece_index] = PieceState::Done;
                }
                PieceCommands::PieceFailed(_peer_id, piece_index) => {
                    self.pieces_status[piece_index] = PieceState::Free;
                }
                PieceCommands::UpdateBitfield(peer_id, index) => {
                    if let Some(peer_info) = self.peers.get_mut(&peer_id) {
                        let index = index as usize;
                        if index > peer_info.bitfield.len() {
                            continue;
                        }
                        peer_info.bitfield[index] = true;
                    }
                }
                PieceCommands::PeerChoked(peer_id) => {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        peer.choked = true;
                    }
                }
                PieceCommands::PeerUnchoke(peer_id) => {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        peer.choked = false;
                    }
                }
                PieceCommands::PeerRegister(peer_id, num_pieces) => {
                    self.peers.insert(
                        peer_id,
                        PeerState {
                            choked: true,
                            interested: false,
                            bitfield: vec![false; num_pieces],
                            outstanding: 0,
                        },
                    );
                }
                PieceCommands::SetBitfield(peer_id, bitfield) => {
                    if let Some(peer_info) = self.peers.get_mut(&peer_id) {
                        peer_info.bitfield = bitfield;
                    }
                }
            }
        }
    }
}
