use core::panic;

use crate::bencode::{BObject, Parser, get_value};
use crate::network::{request_peers_http, request_peers_udp};
use rand::Rng;
use sha1::{Digest, Sha1};

pub fn get_peers(file_path: String) {
    let data = std::fs::read(&file_path).unwrap();
    let mut parser = Parser {
        data: &data,
        pos: 0,
        info_range: None,
    };

    let _parsed = parser.parse_value();

    let hash;
    if let Some((s, e)) = parser.info_range {
        let info_bytes = &parser.data[s..e]; // EXACT bencoded slice
        let mut hasher = Sha1::new();
        hasher.update(info_bytes);
        hash = hasher.finalize();
    } else {
        panic!("Can't get info hash");
    }

    let mut announce_urls = Vec::new();

    match get_value(&_parsed, "announce-list".to_string()) {
        Some(items) => {
            if let BObject::List(byte_list) = items {
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
                panic!("Can't get tracker url");
            }
        }
        None => panic!("Can't get tracker url"),
    };

    let peer_id = gen_peer_id();
    let port = 6881;
    let downloaded = 0;
    let uploaded = 0;
    let left;
    match get_value(&_parsed, "info".to_string()) {
        Some(info) => match get_value(&info, "length".to_string()) {
            Some(v) => {
                if let BObject::Int(e) = v {
                    left = e as u64;
                } else {
                    panic!("Can't get bytes left")
                }
            }
            None => match get_value(&info, "files".to_string()) {
                Some(f) => {
                    let mut sum: i64 = 0;
                    if let BObject::List(files) = f {
                        for file in files {
                            if let BObject::Dict(v) = file
                                && let BObject::Int(len) = v[0].1
                            {
                                sum += len;
                            }
                        }
                    }
                    left = sum as u64;
                }
                None => panic!("Can't get bytes left"),
            },
        },
        None => panic!("Can't get info"),
    }
    let compact = 1;
    let event = "started";
    let numwant = 50;

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
                Ok(_) => break,
                Err(e) => println!("Error(http): {e}"),
            }
        } else if url.starts_with("udp:") {
            // continue;
            match request_peers_udp(
                &url,
                &compute_info_hash(&hash),
                &peer_id,
                left,
                port,
            ) {
                Ok(_) => break,
                Err(e) => println!("Error(udp): {e}"),
            }
        }
    }
    println!("===== DONE {} =====", file_path);
}

fn compute_info_hash(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);

    // This returns GenericArray<u8, 20>
    let result = hasher.finalize();

    // Convert to [u8; 20]
    result.into()
}

fn gen_peer_id() -> [u8; 20] {
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
