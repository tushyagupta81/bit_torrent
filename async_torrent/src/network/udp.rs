use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
};

use tokio::net::UdpSocket;

use crate::peers::Peers;

pub async fn get_peers(
    tracker_url: String,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
    left: u64,
    port: u16,
    downloaded: u64,
    uploaded: u64,
    num_want: u32,
) -> Result<Peers, Box<dyn Error + Send+Sync>> {
    let tracker_addr = tracker_url
        .trim_start_matches("udp://")
        .trim_end_matches("/announce");

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(tracker_addr).await?;

    let protocol_id: i64 = 0x41727101980;
    let connect_action: i32 = 0;
    let connect_tid: i32 = rand::random();

    let mut buf = Vec::new();
    buf.extend_from_slice(&protocol_id.to_be_bytes());
    buf.extend_from_slice(&connect_action.to_be_bytes());
    buf.extend_from_slice(&connect_tid.to_be_bytes());

    socket.send(&buf).await?;

    let mut resp = [0u8; 1024];
    let len = socket.recv(&mut resp).await?;
    if len < 16 {
        return Err("Response too short".into());
    }
    let (action, tid, conn_id) = parse_connect_response(&resp[..len]).map_err(|e| e.to_string())?;

    if tid != connect_tid {
        return Err("Wrong transaction ID".into());
    }
    if action != 0 {
        return Err("Wrong action".into());
    }

    let event: u32 = 2;
    let announce_action: u32 = 1;
    let announce_tid: u32 = rand::random();
    let ip: u32 = 0;
    let key: u32 = rand::random();

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

    socket.send(&announce).await?;

    let len = socket.recv(&mut resp).await?;
    if len < 20 {
        return Err("Response too short".into());
    }
    let (ann_action, ann_tid, _ann_interval, ann_leechers, ann_seeders) =
        parse_announce_response(&resp[..20])?;
    if ann_seeders == 0 {
        return Err("No seeders".into());
    }
    if ann_tid != announce_tid {
        return Err("Wrong transaction ID".into());
    }
    if ann_action != 1 {
        return Err("Wrong action".into());
    }

    println!("Seeders = {ann_seeders}, Leechers = {ann_leechers}");

    let peers = parse_announce_response_peers(&resp[..len])?;

    Ok(peers)
}

fn parse_connect_response(resp: &[u8]) -> Result<(i32, i32, i64), String> {
    if resp.len() < 16 {
        return Err("connect response too short".into());
    }
    let action = i32::from_be_bytes(resp[0..4].try_into().unwrap());
    let transaction_id = i32::from_be_bytes(resp[4..8].try_into().unwrap());
    let connection_id = i64::from_be_bytes(resp[8..16].try_into().unwrap());
    Ok((action, transaction_id, connection_id))
}

fn parse_announce_response(resp: &[u8]) -> Result<(u32, u32, u32, u32, u32), String> {
    let action = u32::from_be_bytes(resp[0..4].try_into().unwrap());
    let transaction_id = u32::from_be_bytes(resp[4..8].try_into().unwrap());
    let interval = u32::from_be_bytes(resp[8..12].try_into().unwrap());
    let leecher = u32::from_be_bytes(resp[12..16].try_into().unwrap());
    let seeder = u32::from_be_bytes(resp[16..20].try_into().unwrap());
    Ok((action, transaction_id, interval, leecher, seeder))
}

fn parse_announce_response_peers(resp: &[u8]) -> Result<Peers, String> {
    let peers_bytes = &resp[20..]; // skip header (20 bytes)
    let peers = Peers(
        peers_bytes
            .chunks(6)
            .map(|chunk| {
                SocketAddrV4::new(
                    Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                    u16::from_be_bytes([chunk[4], chunk[5]]),
                )
            })
            .collect(),
    );
    Ok(peers)
}
