use sha1::{Digest, Sha1};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

#[allow(dead_code)]
pub struct PeerConnection {
    pub ip: String,
    pub port: u16,
    pub socket: TcpStream,
    pub bitfield: Vec<bool>,
}

pub fn intitalize_peer_connections(
    peers: &[Peer],
    info_hash: &[u8; 20],
) -> Result<Vec<PeerConnection>, String> {
    let mut valid_peers = Vec::new();
    for peer in peers {
        println!("Trying peer {}", peer.ip);
        let addr: SocketAddr = format!("{}:{}", peer.ip, peer.port).parse().unwrap();
        let mut socket = match TcpStream::connect_timeout(&addr, Duration::new(5, 0)) {
            Ok(soc) => soc,
            Err(_) => {
                continue;
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let peer_id = gen_peer_id();

        match handshake(&mut socket, info_hash, &peer_id) {
            Ok(_) => {}
            Err(_) => continue,
        };
        // println!("Handshake successful");

        let has_pieces = match read_bitfield(&mut socket) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // println!("Bitfield successful");

        match interested(&mut socket) {
            Ok(_) => {}
            Err(_) => continue,
        };
        // println!("Interested successful");

        match read_unchoke(&mut socket) {
            Ok(_) => {}
            Err(_) => continue,
        };
        // println!("Unchoke successful");

        valid_peers.push(PeerConnection {
            ip: peer.ip.clone(),
            port: peer.port,
            socket,
            bitfield: has_pieces,
        });
        println!("Got peer {}", peer.ip);
    }
    Ok(valid_peers)
}

use crate::{network::Peer, tracker::gen_peer_id};

fn read_message_non_block(socket: &mut TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);

    // If len is 0, it's a keep-alive message
    if len == 0 {
        return Ok((0, vec![]));
    }

    let mut buf = vec![0u8; len as usize];
    socket.read_exact(&mut buf)?;

    let msg_id = buf[0];
    let payload = buf[1..].to_vec();

    Ok((msg_id, payload))
}

fn read_message(socket: &mut TcpStream) -> Result<(u8, Vec<u8>), String> {
    let mut retry = 0;
    loop {
        match read_message_non_block(socket) {
            Ok((msg_id, payload)) => return Ok((msg_id, payload)),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                retry += 1;
                std::thread::sleep(Duration::from_millis(50));
                if retry > 1 {
                    return Err(format!("Failed reading message: {e}"));
                }
                continue;
            }
            Err(e) => return Err(format!("Failed reading message: {e}")),
        }
    }
}

fn interested(socket: &mut TcpStream) -> Result<(), String> {
    let msg_len: [u8; 4] = [0, 0, 0, 1];
    let msg = 2;

    let mut packet = Vec::with_capacity(5);
    packet.extend_from_slice(&msg_len);
    packet.push(msg);

    socket.write_all(&packet).unwrap();

    if let Ok((msg_id, _payload)) = read_message(socket)
        && msg_id != 1
    {
        return Err("Did not get unchoke packet back".into());
    }
    Ok(())
}

fn handshake(
    socket: &mut TcpStream,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
) -> Result<(), String> {
    let pstrlen: u8 = 19;
    let pstr = b"BitTorrent protocol";
    let reserved = [0u8; 8];

    let mut packet = Vec::with_capacity(68);
    packet.push(pstrlen);
    packet.extend_from_slice(pstr);
    packet.extend_from_slice(&reserved);
    packet.extend_from_slice(info_hash);
    packet.extend_from_slice(peer_id);

    socket.write_all(&packet).unwrap();

    // println!("Handshake sent     {:?}", &packet);
    if let Err(err) = socket.read_exact(&mut packet) {
        return Err(format!("failed reading handshake: {err}").to_string());
    }
    // println!("Handshake received {:?}", &packet);

    if packet[0] != pstrlen {
        return Err("Handshake 1st byte did not match".to_string());
    } else if &packet[1..20] != pstr {
        return Err("BitTorrent protocol missing from packet".to_string());
    } else if packet[20..28] != reserved {
        // return Err("Reserved bits not correct".to_string());
    } else if &packet[28..48] != info_hash {
        return Err("Wrong info_hash returned from peer".to_string());
    } else if &packet[48..68] != peer_id {
        return Err("Wrong peer_id returned from peer".to_string());
    }

    Ok(())
}

fn read_bitfield(socket: &mut TcpStream) -> Result<Vec<bool>, String> {
    if let Ok((tag, bitfield_msg)) = read_message(socket) {
        if tag != 5 {
            return Err("Did not get a bitfield message from peer".into());
        }
        return Ok(parse_bitfield(&bitfield_msg[1..]));
    }
    Err("Unable to read bitfield message".into())
}

fn read_unchoke(socket: &mut TcpStream) -> Result<(), String> {
    let (unchoke_msg_id, _unchoke_payload) = match read_message(socket) {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("Error while reciving unchoke message: {e}"));
        }
    };

    if unchoke_msg_id != 1 {
        return Err(format!("Did not recive unchoke, got {unchoke_msg_id}"));
    }
    Ok(())
}

pub fn get_piece_from_peer(
    piece_index: usize,
    piece_len: usize,
    pieces: &[u8],
    peer: &mut PeerConnection,
) -> Result<Vec<u8>, String> {
    let block_len = 16 * 1024;

    if peer.bitfield[piece_index] {
        println!("Requesting piece {piece_index}");

        let piece_size = piece_len;
        let mut piece_buf = vec![0u8; piece_size];

        let mut offset = 0;

        while offset < piece_size {
            let len = std::cmp::min(block_len, piece_size - offset);

            send_request(
                &mut peer.socket,
                piece_index as u32,
                offset as u32,
                len as u32,
            )
            .unwrap();

            let (msg_id, payload) = match read_message(&mut peer.socket) {
                Ok(p) => p,
                Err(e) => {
                    return Err(format!("Unable to read piece message: {e}"));
                }
            };

            if msg_id == 7 {
                let _index = u32::from_be_bytes(payload[0..4].try_into().unwrap());
                let begin = u32::from_be_bytes(payload[4..8].try_into().unwrap());
                let block = payload[8..].to_vec();
                piece_buf[begin as usize..begin as usize + block.len()].copy_from_slice(&block);
            } else if msg_id == 0 {
                println!("Health check/Choke");
                continue;
            } else {
                return Err(format!("Did not recive piece message, got {msg_id}"));
            }

            offset += len;
        }

        if verify_hash(&piece_buf, piece_index, pieces) {
            println!("Piece {} passed hash check", piece_index);
            return Ok(piece_buf);
        } else {
            println!("Piece {} failed hash check", piece_index);
        }
    }

    // send_request(&mut socket, piece_id, 0, 50);

    Err("Unable to get piece from peer".into())
}

pub fn verify_hash(
    piece_buf: &[u8],
    piece_index: usize,
    pieces_field: &[u8], // concatenated 20-byte piece hashes from .torrent metadata
) -> bool {
    // Expected hash from .torrent (20 bytes per piece)
    let start = piece_index * 20;
    let end = start + 20;
    let expected_hash = &pieces_field[start..end];

    // Compute SHA-1 for downloaded piece
    let mut hasher = Sha1::new();
    hasher.update(piece_buf);
    let result = hasher.finalize();

    // Compare digest
    result[..] == expected_hash[..]
}

fn send_request(
    socket: &mut TcpStream,
    piece_index: u32,
    begin: u32,
    block_len: u32,
) -> std::io::Result<()> {
    let mut msg = Vec::with_capacity(17);
    msg.extend(&(13u32.to_be_bytes())); // length
    msg.push(6u8); // message ID = request
    msg.extend(&piece_index.to_be_bytes());
    msg.extend(&begin.to_be_bytes());
    msg.extend(&block_len.to_be_bytes());

    socket.write_all(&msg)?;
    Ok(())
}

fn parse_bitfield(payload: &[u8]) -> Vec<bool> {
    let mut pieces = Vec::new();
    for byte in payload.iter() {
        for bit in 0..8 {
            if byte & (1 << (7 - bit)) != 0 {
                pieces.push(true);
            } else {
                pieces.push(false);
            }
        }
    }
    pieces
}
