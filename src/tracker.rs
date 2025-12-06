use core::panic;
use std::error::Error;

use crate::bencode::{BObject, Parser, get_value};
use crate::network::{Peer, request_peers_http, request_peers_udp};
use rand::Rng;
use sha1::{Digest, Sha1};

#[derive(Debug)]
pub struct FileInfo {
    pub name: String,
    pub size: i64,
}

#[derive(Debug)]
pub struct Torrent {
    pub peers: Vec<Peer>,
    pub info_hash: [u8; 20],
    pub piece_len: i64,
    pub num_pieces: usize,
    pub pieces: Vec<u8>,
    pub files: Vec<FileInfo>,
}

pub fn get_peers(file_path: String) -> Result<Torrent, Box<dyn Error>> {
    let data = std::fs::read(&file_path).unwrap();
    let mut parser = Parser {
        data: &data,
        pos: 0,
        info_range: None,
    };

    let _parsed = parser.parse_value();

    let hash: [u8; 20];
    if let Some((s, e)) = parser.info_range {
        let info_bytes = &parser.data[s..e]; // EXACT bencoded slice
        let mut hasher = Sha1::new();
        hasher.update(info_bytes);
        hash = hasher.finalize().into();
    } else {
        panic!("Can't get info hash");
    }

    let mut announce_urls = Vec::new();

    if let Some(BObject::List(byte_list)) = get_value(&_parsed, "announce-list".to_string()) {
        for entry in byte_list {
            if let BObject::List(bl) = entry {
                for bytes in bl {
                    if let BObject::Str(url) = bytes {
                        announce_urls.push(String::from_utf8_lossy(&url).to_string());
                    }
                }
            }
        }
    } else {
        match get_value(&_parsed, "announce".to_string()) {
            Some(bytes) => {
                if let BObject::Str(url) = bytes {
                    announce_urls.push(String::from_utf8_lossy(&url).to_string());
                } else {
                    panic!("Can't get tracker url");
                }
            }
            None => panic!("Can't get tracker url"),
        };
    }

    let peer_id = gen_peer_id();
    let port = 6881;
    let downloaded = 0;
    let uploaded = 0;
    let left;
    let piece_len;
    let num_pieces;
    let pieces;
    let mut files_list = Vec::new();
    match get_value(&_parsed, "info".to_string()) {
        Some(info) => {
            match get_value(&info, "length".to_string()) {
                Some(v) => {
                    if let BObject::Int(e) = v {
                        left = e;
                        match get_value(&info, "name".to_string()) {
                            Some(BObject::Str(name)) => {
                                let file_name = String::from_utf8(name).unwrap();
                                files_list.push(FileInfo {
                                    name: file_name,
                                    size: e,
                                });
                            }
                            None | Some(_) => {
                                panic!("Can't find file name for single file");
                            }
                        }
                    } else {
                        panic!("Can't get bytes left")
                    }
                }
                None => match get_value(&info, "files".to_string()) {
                    Some(f) => {
                        let mut sum: i64 = 0;
                        if let BObject::List(files) = f {
                            for file in files {
                                if let BObject::Dict(v) = file {
                                    let mut size = 0;
                                    let mut file_name = String::from("");
                                    if let BObject::Int(len) = v[0].1 {
                                        sum += len;
                                        size = len;
                                    }
                                    if let BObject::List(names) = &v[1].1 {
                                        for name in names {
                                            if let BObject::Str(n) = name {
                                                file_name += str::from_utf8(n).unwrap();
                                                file_name += "/";
                                            }
                                        }
                                    }
                                    files_list.push(FileInfo {
                                        name: file_name.trim_matches('/').to_string(),
                                        size,
                                    });
                                }
                            }
                        }
                        left = sum;
                    }
                    None => panic!("Can't get bytes left"),
                },
            }
            match get_value(&info, "piece length".to_string()) {
                Some(v) => {
                    if let BObject::Int(e) = v {
                        piece_len = e;
                    } else {
                        panic!("Can't get piece length")
                    }
                }
                None => panic!("Can't get piece length"),
            }
            match get_value(&info, "pieces".to_string()) {
                Some(v) => {
                    if let BObject::Str(e) = v {
                        num_pieces = e.len().div_ceil(20);
                        pieces = e;
                    } else {
                        panic!("Can't get number of pieces")
                    }
                }
                None => panic!("Can't get number of pieces"),
            }
        }
        None => panic!("Can't get info"),
    }
    let compact = 1;
    let event = "started";
    let numwant = 50;

    // println!("{:?}", &hash);

    for url in announce_urls {
        // manually add info_hash and peer_id to avoid double encoding
        let query = format!(
            "info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact={}&event={}&numwant={}",
            encode_binary(&hash),    // percent-encoded already
            encode_binary(&peer_id), // percent-encoded already
            port,
            uploaded,
            downloaded,
            left,
            compact,
            event,
            numwant
        );

        // attach query to base URL
        let full_url = format!("{}?{}", url, query);

        if url.to_string().starts_with("http") {
            match request_peers_http(&full_url) {
                Ok(peers) => {
                    return Ok(Torrent {
                        peers,
                        info_hash: hash,
                        piece_len,
                        num_pieces,
                        pieces,
                        files: files_list,
                    });
                }
                Err(e) => eprintln!("Error(http): {e}"),
            }
        } else if url.starts_with("udp:") {
            match request_peers_udp(&url, &hash, &peer_id, left, port) {
                Ok(peers) => {
                    return Ok(Torrent {
                        peers,
                        info_hash: hash,
                        piece_len,
                        num_pieces,
                        pieces,
                        files: files_list,
                    });
                }
                Err(e) => eprintln!("Error(udp): {e}"),
            }
        }
    }
    Err("Can't get peers from any tracker".into())
}

pub fn gen_peer_id() -> [u8; 20] {
    let mut id = *b"-RS0001-000000000000";
    let mut rng = rand::rng();
    for i in id.iter_mut().skip(8) {
        *i = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
            [rng.random_range(0..62)];
    }
    id
}

fn encode_binary(data: &[u8]) -> String {
    use url::form_urlencoded::byte_serialize;
    byte_serialize(data).collect()
}
