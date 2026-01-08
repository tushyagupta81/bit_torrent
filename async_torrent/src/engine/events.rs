pub enum UiEvent {
    PieceRequested(usize),
    PieceDownloading(usize),
    PieceCompleted(usize),

    PeerUpdate {
        peer_id: String,
        task: String,
        choked: bool,
    },
    PeerDisconnected(String),
}
