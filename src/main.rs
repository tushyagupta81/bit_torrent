mod bencode;
mod download;
mod io;
mod network;
mod peers;
mod tracker;

use crate::download::download;

fn main() {
    // download("./torrents/wired-cd.torrent".to_string())
    download("./torrents/test.torrent".to_string())
    // let torrent_dir = "./torrents";
    // process_torrent_dir(torrent_dir);
}
