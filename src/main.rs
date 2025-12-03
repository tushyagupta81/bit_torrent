mod bencode;
mod network;
mod tracker;

use crate::tracker::get_peers;
use std::fs;
use std::path::Path;

fn main() {
    // get_peers("./torrents/sample.torrent".to_string());
    let torrent_dir = "./torrents";
    process_torrent_dir(torrent_dir);
}

fn process_torrent_dir(dir: &str) {
    let path = Path::new(dir);

    if !path.exists() {
        eprintln!("Directory does not exist: {}", dir);
        return;
    }

    // Iterate through directory entries
    for entry in fs::read_dir(path).expect("Unable to read torrent directory") {
        let file_path = entry.unwrap().path();

        // Only process .torrent files
        if let Some(ext) = file_path.extension()
            && ext == "torrent"
            && let Some(path_str) = file_path.to_str()
        {
            println!("=== {path_str} ===");
            match get_peers(path_str.to_string()) {
                Ok(peers) => println!("{:?}", peers),
                Err(e) => eprintln!("Error: {e}"),
            }
            println!("\n\n");
        }
    }
}
