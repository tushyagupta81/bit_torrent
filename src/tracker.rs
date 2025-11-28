use core::panic;

use crate::bencode::{BObject, Parser, get_value};
use rand::Rng;

pub fn get_peers(file_path: String) {
    let data = std::fs::read(file_path).unwrap();
    let mut parser = Parser {
        data: &data,
        pos: 0,
        info_range: None,
    };

    let _parsed = parser.parse_value();

    let hash;
    if let Some((s, e)) = parser.info_range {
        let info_bytes = &parser.data[s..e]; // EXACT bencoded slice
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(info_bytes);
        hash = hasher.finalize();
    } else {
        panic!("Can't get info hash");
    }

    let announce_url = match get_value(&_parsed, "announce".to_string()) {
        Some(value) => {
            if let BObject::Str(bytes) = value {
                String::from_utf8_lossy(&bytes).to_string()
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

    let mut url = reqwest::Url::parse(&announce_url).unwrap();

    // manually add info_hash and peer_id to avoid double encoding
    let mut query = format!(
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
        request_peers(&full_url);
    }
    dbg!(full_url);
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

fn request_peers(url: &String) {
    let response = reqwest::blocking::get(url)
        .expect("Failed to send request to tracker")
        .bytes()
        .expect("Failed to read response from tracker");

    println!("Response from tracker: {response:?}");
}
