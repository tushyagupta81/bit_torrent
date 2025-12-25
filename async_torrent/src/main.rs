use std::sync::{Arc, RwLock};

use tokio::task::JoinSet;

use crate::{
    bencode::decode_bencode, files::initialize_files, peers::{PieceState, peer_download}, tracker::fetch_peers
};

mod bencode;
mod files;
mod network;
mod peers;
mod tracker;
mod utils;

#[tokio::main]
async fn main() {
    // let info = decode_bencode("../torrents/one-piece.torrent".to_string()).unwrap();
    let info = decode_bencode("../torrents/wired-cd.torrent".to_string()).unwrap();
    // let info = decode_bencode("../torrents/test.torrent".to_string()).unwrap();


    match initialize_files(&info.info) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error while initializing files: {e}");
            return;
        }
    };

    let peers = fetch_peers(&info).await.unwrap();

    let pieces_done = Arc::new(RwLock::new(vec![
        PieceState::Free;
        info.info.pieces.len().div_ceil(20)
    ]));

    let info_ptr = Arc::new(info);

    let mut join_set = JoinSet::new();

    for peer in peers.peers.0 {
        let pieces_done = pieces_done.clone();
        let info_ptr = info_ptr.clone();

        join_set.spawn(async move {
            println!("Trying peer {}", peer);
            if let Err(e) = peer_download(peer, pieces_done, info_ptr).await {
                eprintln!("{e}");
            }
        });
    }

    while let Some(res) = join_set.join_next().await {
        res.expect("task panicked");
    }
}
