mod bencode;
mod download;
mod network;
mod peers;
mod tracker;

use crate::download::download;

fn main() {
    download("./torrents/test.torrent".to_string())
    // let torrent_dir = "./torrents";
    // process_torrent_dir(torrent_dir);
}
