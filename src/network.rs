use crate::bencode::{BObject, Parser, get_value};
use std::{net::UdpSocket, time::Duration};

fn parse_connect_response(resp: &[u8]) -> Result<(i32, i32, i64), String> {
    if resp.len() < 16 {
        return Err("connect response too short".into());
    }
    let action = i32::from_be_bytes(resp[0..4].try_into().unwrap());
    let transaction_id = i32::from_be_bytes(resp[4..8].try_into().unwrap());
    let connection_id = i64::from_be_bytes(resp[8..16].try_into().unwrap());
    Ok((action, transaction_id, connection_id))
}

fn parse_announce_response(resp: &[u8]) -> Result<Vec<(String, u16)>, String> {
    if resp.len() < 20 {
        return Err("announce response too short".into());
    }
    let action = i32::from_be_bytes(resp[0..4].try_into().unwrap());
    if action != 1 {
        return Err(format!("Not an announce response: {}", action));
    }

    let peers_bytes = &resp[20..]; // skip header (20 bytes)
    let peers = peers_bytes
        .chunks(6)
        .map(|chunk| {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            (ip, port)
        })
        .collect();
    Ok(peers)
}

#[derive(Debug)]
pub struct Peer {
    ip: String,
    peer_id: String,
    port: u64,
}

pub fn request_peers_http(url: &String) -> Result<Vec<Peer>, reqwest::Error> {
    let response = reqwest::blocking::get(url)?.bytes()?;

    let mut parser = Parser {
        data: &response,
        pos: 0,
        info_range: None,
    };
    let root = parser.parse_value(); // will parse the whole dictionary

    // Get the peers field
    let mut peer_list = Vec::new();
    if let Some(BObject::List(peers)) = get_value(&root, "peers".to_string()) {
        for peer in peers {
            if let BObject::Dict(info) = peer {
                let mut p = Peer {
                    ip: "".to_string(),
                    peer_id: "".to_string(),
                    port: 0,
                };
                for item in info {
                    match item.0.as_str() {
                        "ip" => {
                            p.ip = if let BObject::Str(i) = item.1 {
                                String::from_utf8_lossy(&i).to_string()
                            } else {
                                panic!("Unable to fetch ip");
                            }
                        }
                        "peer id" => {
                            p.peer_id = if let BObject::Str(i) = item.1 {
                                String::from_utf8_lossy(&i).to_string()
                            } else {
                                panic!("Unable to peer id");
                            }
                        }
                        "port" => {
                            p.port = if let BObject::Int(i) = item.1 {
                                i as u64
                            } else {
                                panic!("Unable to peer id");
                            }
                        }
                        _ => (),
                    }
                }
                peer_list.push(p);
            }
        }
    }

    Ok(peer_list)
}

pub fn request_peers_udp(
    tracker_url: &str,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
    left: u64,
    port: u16,
) -> Result<Vec<(String, u16)>, String> {
    // remove scheme & /announce
    let tracker_addr = tracker_url
        .trim_start_matches("udp://")
        .trim_end_matches("/announce");

    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket.connect(tracker_addr).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;

    // --- CONNECT ---
    let protocol_id: i64 = 0x41727101980;
    let connect_action: i32 = 0;
    let connect_tid: i32 = rand::random();

    let mut buf = Vec::new();
    buf.extend_from_slice(&protocol_id.to_be_bytes());
    buf.extend_from_slice(&connect_action.to_be_bytes());
    buf.extend_from_slice(&connect_tid.to_be_bytes());

    socket.send(&buf).map_err(|e| e.to_string())?;

    let mut resp = [0u8; 1024];
    let len = socket.recv(&mut resp).map_err(|e| e.to_string())?;
    let (_action, _tid, conn_id) =
        parse_connect_response(&resp[..len]).map_err(|e| e.to_string())?;

    // --- ANNOUNCE ---
    let announce_action: i32 = 1;
    let announce_tid: i32 = rand::random();
    let downloaded: i64 = 0;
    let uploaded: i64 = 0;
    let event: i32 = 2; // started
    let ip: i32 = 0; // default
    let key: i32 = rand::random();
    let num_want: i32 = 50;

    let mut announce = Vec::new();
    announce.extend_from_slice(&conn_id.to_be_bytes());
    announce.extend_from_slice(&announce_action.to_be_bytes());
    announce.extend_from_slice(&announce_tid.to_be_bytes());
    announce.extend_from_slice(info_hash);
    announce.extend_from_slice(peer_id);
    announce.extend_from_slice(&downloaded.to_be_bytes());
    announce.extend_from_slice(&left.to_be_bytes());
    announce.extend_from_slice(&uploaded.to_be_bytes());
    announce.extend_from_slice(&event.to_be_bytes());
    announce.extend_from_slice(&ip.to_be_bytes());
    announce.extend_from_slice(&key.to_be_bytes());
    announce.extend_from_slice(&num_want.to_be_bytes());
    announce.extend_from_slice(&port.to_be_bytes());

    socket.send(&announce).map_err(|e| e.to_string())?;

    let len = socket.recv(&mut resp).map_err(|e| e.to_string())?;
    let peers = parse_announce_response(&resp[..len])?;

    for (ip, port) in &peers {
        println!("{} {}", ip, port);
    }

    Ok(peers)
}

fn parse_peers_udp(peer_bytes: &[u8]) -> Vec<(String, u16)> {
    peer_bytes
        .chunks(6)
        .map(|chunk| {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            (ip, port)
        })
        .collect()
}
