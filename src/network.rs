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

fn parse_announce_response(resp: &[u8]) -> Result<(i32, i32, i32, i32, i32), String> {
    let action = i32::from_be_bytes(resp[0..4].try_into().unwrap());
    let transaction_id = i32::from_be_bytes(resp[4..8].try_into().unwrap());
    let interval = i32::from_be_bytes(resp[8..12].try_into().unwrap());
    let leecher = i32::from_be_bytes(resp[12..16].try_into().unwrap());
    let seeder = i32::from_be_bytes(resp[16..20].try_into().unwrap());
    Ok((action, transaction_id, interval, leecher, seeder))
}

fn parse_announce_response_peers(resp: &[u8]) -> Result<Vec<Peer>, String> {
    let peers_bytes = &resp[20..]; // skip header (20 bytes)
    let peers = peers_bytes
        .chunks(6)
        .map(|chunk| {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            Peer {
                ip,
                peer_id: None,
                port,
            }
        })
        .collect();
    Ok(peers)
}

#[derive(Debug)]
pub struct Peer {
    pub ip: String,
    peer_id: Option<String>,
    pub port: u16,
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
                    peer_id: None,
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
                                Some(String::from_utf8_lossy(&i).to_string())
                            } else {
                                panic!("Unable to peer id");
                            }
                        }
                        "port" => {
                            p.port = if let BObject::Int(i) = item.1 {
                                i as u16
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
    left: i64,
    port: u16,
) -> Result<Vec<Peer>, String> {
    // remove scheme & /announce
    let tracker_addr = tracker_url
        .trim_start_matches("udp://")
        .trim_end_matches("/announce");

    let socket = match UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string()) {
        Ok(soc) => soc,
        Err(e) => {
            eprintln!("Unable to bind socket");
            return Err(e);
        }
    };
    match socket.connect(tracker_addr).map_err(|e| e.to_string()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Unable to connect to tracker");
            return Err(e);
        }
    };
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

    match socket.send(&buf).map_err(|e| e.to_string()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Unable to send data to UDP tracker");
            return Err(e);
        }
    };

    let mut resp = [0u8; 1024];
    let len = match socket.recv(&mut resp).map_err(|e| e.to_string()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error while reciving connect response from UDP tracker");
            return Err(e);
        }
    };
    if len < 16 {
        eprintln!("Connect respose length has to be atleast 16 bytes");
        return Err("Response too short".to_string());
    }
    let (_action, _tid, conn_id) =
        match parse_connect_response(&resp[..len]).map_err(|e| e.to_string()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Unable to parse connect response");
                return Err(e);
            }
        };

    if _tid != connect_tid {
        eprintln!("Got back wrong Transaction id from tracker");
        return Err("Wrong transaction ID".to_string());
    }
    if _action != 0 {
        eprintln!("Got back wrong action from tracker");
        return Err("Wrong action".to_string());
    }

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

    let len = match socket.recv(&mut resp).map_err(|e| e.to_string()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error while reciving peers from UDP tracker");
            return Err(e);
        }
    };

    if len < 20 {
        eprintln!("Announce respose length has to be atleast 20 bytes");
        return Err("Response too short".to_string());
    }
    let (ann_action, ann_tid, _ann_interval, _ann_leechers, ann_seeders) =
        match parse_announce_response(&resp[..20]) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Unable to parse announce response");
                return Err(e);
            }
        };
    if ann_seeders == 0 {
        return Err("No seeders".to_string());
    }
    if ann_tid != announce_tid {
        eprintln!("Got back wrong Transaction id from tracker");
        return Err("Wrong transaction ID".to_string());
    }
    if ann_action != 1 {
        eprintln!("Got back wrong action from tracker");
        return Err("Wrong action".to_string());
    }

    let peers = parse_announce_response_peers(&resp[..len])?;

    Ok(peers)
}
