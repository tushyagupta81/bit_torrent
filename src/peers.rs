use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use crate::tracker::gen_peer_id;

fn read_message(socket: &mut TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
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
        println!("Reserved bits not correct");
        // return Err("Reserved bits not correct".to_string());
    } else if &packet[28..48] != info_hash {
        return Err("Wrong info_hash returned from peer".to_string());
    } else if &packet[48..68] != peer_id {
        return Err("Wrong peer_id returned from peer".to_string());
    }

    Ok(())
}

pub fn get_piece_from_peer(
    ip: String,
    port: u16,
    info_hash: &[u8; 20],
    piece_len: i64,
    piece_index: usize,
) -> Result<(), String> {
    let mut socket = TcpStream::connect(format!("{}:{}", ip, port)).unwrap();
    socket.set_write_timeout(Some(Duration::new(5, 0))).unwrap();
    socket.set_read_timeout(Some(Duration::new(5, 0))).unwrap();
    let peer_id = gen_peer_id();

    handshake(&mut socket, info_hash, &peer_id)?;

    let mut msg_len = [0u8; 4];
    socket.read_exact(&mut msg_len).unwrap();
    let len = u32::from_be_bytes(msg_len);

    let mut bitfield_msg = vec![0u8; len as usize];
    socket.read_exact(&mut bitfield_msg).unwrap();

    let tag = bitfield_msg[0];
    if tag != 5 {
        return Err("Did not get a bitfield message from peer".into());
    }
    let pieces = parse_bitfield(&bitfield_msg[1..]);

    if !pieces.contains(&piece_index) {
        return Err("Piece not with peer".into());
    }

    match interested(&mut socket) {
        Ok(_) => {}
        Err(e) => {
            println!("Error while setting interested: {e}");
        }
    };

    // send_request(&mut socket, piece_id, 0, 50);


    Ok(())
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

fn parse_bitfield(payload: &[u8]) -> Vec<usize> {
    let mut pieces = Vec::new();
    for (byte_index, byte) in payload.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << (7 - bit)) != 0 {
                let piece_index = byte_index * 8 + bit;
                pieces.push(piece_index);
            }
        }
    }
    pieces
}
