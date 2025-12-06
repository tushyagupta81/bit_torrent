mod bencode;
mod io;
mod download;
mod network;
mod peers;
mod tracker;

use crate::download::download;

fn main() {
    download("./torrent_files/test_file.torrent".to_string())
    // let torrent_dir = "./torrents";
    // process_torrent_dir(torrent_dir);
}
