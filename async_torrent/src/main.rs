use std::sync::Arc;

use tokio::{sync::mpsc, task::JoinSet};

use crate::{
    bencode::decode_bencode, central_manager::CentralManager, files::initialize_files,
    peers_task::Peer, tracker::fetch_peers,
};

mod bencode;
mod central_manager;
mod files;
mod network;
mod peers;
mod peers_task;
mod tracker;
mod utils;

#[tokio::main]
async fn main() {
    // let info = decode_bencode("../torrents/one-piece.torrent".to_string()).unwrap();
    let info = decode_bencode("../torrents/big-buck-bunny.torrent".to_string()).unwrap();
    // let info = decode_bencode("../torrents/small.torrent".to_string()).unwrap();
    // let info = decode_bencode("../torrents/wired-cd.torrent".to_string()).unwrap();
    // let info = decode_bencode("../torrents/test.torrent".to_string()).unwrap();

    match initialize_files(&info.info) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error while initializing files: {e}");
            return;
        }
    };

    let peers = fetch_peers(&info).await.unwrap();

    let central = CentralManager::new(info.info.pieces.len().div_ceil(20) as usize);

    println!("Piece = {}", info.info.pieces.len().div_ceil(20));

    let info_ptr = Arc::new(info);

    let mut join_set = JoinSet::new();

    let (mpsc_sender, mpsc_recevier) = mpsc::channel(100);

    join_set.spawn(async move {
        central.run(mpsc_recevier).await;
    });

    for peer in peers.peers.0 {
        let info_ptr = info_ptr.clone();
        let sender = mpsc_sender.clone();

        join_set.spawn(async move {
            if let Ok(mut peer_str) = Peer::new(peer, info_ptr, sender).await {
                match peer_str.start().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error {e}");
                    }
                }
            }
        });
    }

    while let Some(res) = join_set.join_next().await {
        res.expect("task panicked");
    }
}
