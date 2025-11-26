mod bencode;

use bencode::decode;

fn main() {
    // let _ = decode("./torrents/big-buck-bunny.torrent".to_string());
    let _ = decode("./torrents/tears-of-steel.torrent".to_string());
}
