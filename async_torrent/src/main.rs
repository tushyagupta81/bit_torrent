use crate::{bencode::decode_bencode, tracker::fetch_peers};

mod bencode;
mod tracker;
mod utils;
mod network;
mod peers;

#[tokio::main]
async fn main() {
    let info = decode_bencode("../torrents/one-piece.torrent".to_string()).unwrap();
    fetch_peers(&info).await.unwrap();
}
