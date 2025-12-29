use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};
use serde_bencoded::to_vec;
use sha1::{Digest, Sha1};
use std::error::Error;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::unix::fs::FileExt;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::bencode::{FileMode, Info, MetaInfo};
use crate::utils::{gen_peer_id, sha1_hash};

#[derive(Debug, Clone)]
pub struct Peers(pub Vec<SocketAddrV4>);
struct PeersVisitor;

impl<'de> Visitor<'de> for PeersVisitor {
    type Value = Peers;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("6 bytes, the first 4 bytes are a peer's IP address and the last 2 are a peer's port number")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if !v.len().is_multiple_of(6) {
            return Err(E::custom(format!("length is {}", v.len())));
        }
        // TODO: use array_chunks when stable; then we can also pattern-match in closure args
        Ok(Peers(
            v.chunks_exact(6)
                .map(|slice_6| {
                    SocketAddrV4::new(
                        Ipv4Addr::new(slice_6[0], slice_6[1], slice_6[2], slice_6[3]),
                        u16::from_be_bytes([slice_6[4], slice_6[5]]),
                    )
                })
                .collect(),
        ))
    }
}

impl<'de> Deserialize<'de> for Peers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(PeersVisitor)
    }
}

impl Serialize for Peers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut single_slice = Vec::with_capacity(6 * self.0.len());
        for peer in &self.0 {
            single_slice.extend(peer.ip().octets());
            single_slice.extend(peer.port().to_be_bytes());
        }
        serializer.serialize_bytes(&single_slice)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceState {
    Free,
    Done,
    Reserved,
}

pub async fn peer_download(
    peer_addr: SocketAddrV4,
    pieces_done: Arc<RwLock<Vec<PieceState>>>,
    info: Arc<MetaInfo>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let num_pieces = info.info.pieces.len().div_ceil(20);
    let info_portion = &info.info;
    let raw_hash = to_vec(info_portion)?;
    let info_hash = sha1_hash(&raw_hash);

    let mut socket = TcpStream::connect(peer_addr).await?;
    // println!("Connected to {}", peer_addr);
    let peer_id = gen_peer_id();

    handshake(&mut socket, &info_hash, &peer_id).await?;
    // println!("Handshake success");

    let bitfield = read_bitfield(&mut socket).await?;
    // println!("Got bitfield");

    interested(&mut socket).await?;
    // println!("Sent Internested");

    read_unchoke(&mut socket).await?;
    // println!("Got unchoked");

    println!("Got peer {peer_addr}");

    let mut bad_index = num_pieces + 5;
    let mut retry = 0;
    loop {
        if retry > 5 {
            break;
        }
        let piece_index = {
            let pd = pieces_done.read().unwrap();
            let mut index = pd
                .iter()
                .enumerate()
                .find(|(i, state)| **state == PieceState::Free && bitfield[*i])
                .map(|(i, _)| i);
            if index.is_none() {
                index = pd
                    .iter()
                    .enumerate()
                    .find(|(i, state)| {
                        **state == PieceState::Reserved && bitfield[*i] && *i != bad_index
                    })
                    .map(|(i, _)| i)
            }
            if index.is_none() {
                break;
            }
            index.unwrap()
        };

        {
            let mut pd = pieces_done.write().unwrap();
            if pd[piece_index] == PieceState::Free {
                pd[piece_index] = PieceState::Reserved;
            } else {
                continue;
            }
        }
        println!(
            "Starting piece {}/{} for {}",
            piece_index + 1,
            num_pieces,
            peer_addr
        );
        let piece_result = get_piece_from_peer(
            piece_index,
            info.info.piece_length,
            &info.info.pieces,
            &bitfield,
            &mut socket,
        )
        .await;

        if piece_result.is_err() {
            let mut pd = pieces_done.write().unwrap();
            pd[piece_index] = PieceState::Free;
            bad_index = piece_index;
            retry += 1;
            continue;
        }

        let piece = piece_result.unwrap();

        let write_status = write_piece_to_files(&info.info, piece_index, &piece);

        if write_status.is_err() {
            let mut pd = pieces_done.write().unwrap();
            pd[piece_index] = PieceState::Free;
            bad_index = piece_index;
            retry += 1;
            continue;
        }

        println!(
            "\t\t\t\t\tPeer {} finished piece {}/{}",
            peer_addr,
            piece_index + 1,
            num_pieces
        );

        let _ = send_have(&mut socket, piece_index as u32).await;

        {
            let mut pd = pieces_done.write().unwrap();
            pd[piece_index] = PieceState::Done;
        }
    }

    Ok(())
}

fn write_piece_to_files(info: &Info, piece_index: usize, piece_buf: &[u8]) -> std::io::Result<()> {
    let piece_len = info.piece_length;
    let mut piece_offset = piece_index as u64 * piece_len;
    let mut remaining = piece_buf;

    match &info.mode {
        FileMode::MultipleFiles { files } => {
            for file in files {
                if piece_offset >= file.length {
                    piece_offset -= file.length;
                    continue;
                }

                let write_len =
                    std::cmp::min(remaining.len() as u64, file.length - piece_offset) as usize;

                let file_name = format!("{}/{}", info.name, file.path.join("/"));

                {
                    let f = std::fs::File::open(file_name).unwrap();
                    f.try_lock().unwrap();
                    f.write_at(&remaining[..write_len], piece_offset)?;
                    f.unlock().unwrap();
                }

                remaining = &remaining[write_len..];
                piece_offset = 0;

                if remaining.is_empty() {
                    break;
                }
            }
        }
        FileMode::SingleFile { length } => {
            let write_len = std::cmp::min(remaining.len() as u64, length - piece_offset) as usize;
            {
                let f = std::fs::File::open(&info.name).unwrap();
                f.try_lock().unwrap();
                f.write_at(&remaining[..write_len], piece_offset)?;
                f.unlock().unwrap();
            }
            remaining = &remaining[write_len..];
        }
    }

    if !remaining.is_empty() {
        eprintln!("Warning: piece data exceeds file boundaries!");
    }

    Ok(())
}

async fn get_piece_from_peer(
    piece_index: usize,
    piece_len: u64,
    pieces: &[u8],
    bitfield: &[bool],
    socket: &mut TcpStream,
) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let block_len = 16 * 1024;

    if bitfield[piece_index] {
        let piece_size = piece_len;
        let mut piece_buf = vec![0u8; piece_size as usize];

        let mut offset = 0;

        while offset < piece_size {
            let len = std::cmp::min(block_len, piece_size - offset);

            send_request(socket, piece_index as u32, offset as u32, len as u32).await?;

            let (msg_id, payload) = match read_message(socket).await {
                Ok(p) => p,
                Err(e) => {
                    return Err(format!("Unable to read piece message: {e}").into());
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
                return Err(format!("Did not recive piece message, got {msg_id}").into());
            }

            offset += len;
        }

        if verify_hash(&piece_buf, piece_index, pieces) {
            return Ok(piece_buf);
        } else {
            println!("Piece {} failed hash check", piece_index);
        }
    }

    Err("Unable to get piece from peer".into())
}

async fn read_message(socket: &mut TcpStream) -> std::io::Result<(u8, Vec<u8>)> {
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);

    // If len is 0, it's a keep-alive message
    if len == 0 {
        return Ok((0, vec![]));
    }

    let mut buf = vec![0u8; len as usize];
    socket.read_exact(&mut buf).await?;

    let msg_id = buf[0];
    let payload = buf[1..].to_vec();

    Ok((msg_id, payload))
}

async fn interested(socket: &mut TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let msg_len: [u8; 4] = [0, 0, 0, 1];
    let msg = 2;

    let mut packet = Vec::with_capacity(5);
    packet.extend_from_slice(&msg_len);
    packet.push(msg);

    socket.write_all(&packet).await?;

    if let Ok((msg_id, _payload)) = read_message(socket).await
        && msg_id != 1
    {
        return Err("Did not get unchoke packet back".into());
    }
    Ok(())
}

async fn send_have(socket: &mut TcpStream, piece_index: u32) -> std::io::Result<()> {
    let mut buf = [0u8; 9];
    buf[0..4].copy_from_slice(&5u32.to_be_bytes());
    buf[4] = 4;
    buf[5..9].copy_from_slice(&piece_index.to_be_bytes());

    socket.write_all(&buf).await?;
    Ok(())
}

async fn handshake(
    socket: &mut TcpStream,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let pstrlen: u8 = 19;
    let pstr = b"BitTorrent protocol";
    let reserved = [0u8; 8];

    let mut packet = Vec::with_capacity(68);
    packet.push(pstrlen);
    packet.extend_from_slice(pstr);
    packet.extend_from_slice(&reserved);
    packet.extend_from_slice(info_hash);
    packet.extend_from_slice(peer_id);

    socket.write_all(&packet).await?;

    let mut resp = [0u8; 68];
    // println!("Handshake sent     {:?}", &packet);
    if let Err(err) = socket.read_exact(&mut resp).await {
        return Err(format!("failed reading handshake: {err}").into());
    }
    // println!("Handshake received {:?}", &packet);

    if resp[0] != pstrlen {
        return Err("Handshake 1st byte did not match".into());
    } else if &resp[1..20] != pstr {
        return Err("BitTorrent protocol missing from resp".into());
    } else if resp[20..28] != reserved {
        // return Err("Reserved bits not correct".into());
    } else if &resp[28..48] != info_hash {
        return Err("Wrong info_hash returned from peer".into());
    } else if &resp[48..68] != peer_id {
        // return Err("Wrong peer_id returned from peer".into());
    }

    Ok(())
}

async fn read_bitfield(socket: &mut TcpStream) -> Result<Vec<bool>, Box<dyn Error + Send + Sync>> {
    if let Ok((tag, bitfield_msg)) = read_message(socket).await {
        if tag != 5 {
            return Err("Did not get a bitfield message from peer".into());
        }
        return Ok(parse_bitfield(&bitfield_msg));
    }
    Err("Unable to read bitfield message".into())
}

async fn read_unchoke(socket: &mut TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (unchoke_msg_id, _unchoke_payload) = match read_message(socket).await {
        Ok(p) => p,
        Err(e) => {
            return Err(format!("Error while reciving unchoke message: {e}").into());
        }
    };

    if unchoke_msg_id != 1 {
        return Err(format!("Did not recive unchoke, got {unchoke_msg_id}").into());
    }
    Ok(())
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

async fn send_request(
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

    socket.write_all(&msg).await?;
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
