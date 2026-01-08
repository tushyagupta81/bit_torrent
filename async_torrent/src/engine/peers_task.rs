use sha1::{Digest, Sha1};
use std::{
    collections::HashMap, error::Error, net::SocketAddrV4, os::unix::fs::FileExt, sync::Arc,
    time::Duration,
};

use serde_bencoded::to_vec;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, oneshot},
    time::{sleep, timeout},
};

type AsyncError = Box<dyn Error + Send + Sync>;

const TIMEOUT: u64 = 5;
const CHOKE_TIMEOUT: u64 = 30;
const REQUEST_ONCE: u8 = 5;
const BLOCK_LEN: u32 = 16 * 1024;

use crate::{
    bencode::{FileMode, Info, MetaInfo},
    engine::{
        central_manager::{PieceCommands, PieceState},
        events::UiEvent,
    },
    utils::{gen_peer_id, sha1_hash},
};

#[allow(unused)]
pub struct Peer {
    address: SocketAddrV4,
    info: Arc<MetaInfo>,
    num_pieces: usize,
    info_hash: [u8; 20],
    socket: TcpStream,
    total_size: u64,
    bitfield: Vec<bool>,
    outstanding: HashMap<usize, PieceStatus>,
    pub peer_id: [u8; 20],
    pub sender: mpsc::Sender<PieceCommands>,
    pub ui_tx: mpsc::Sender<UiEvent>,
}

struct PieceStatus {
    data: Vec<u8>,
    blocks: Vec<bool>,
    received: usize,
}

#[derive(PartialEq, Eq)]
enum MsgType {
    Choke = 0,
    Unchoke = 1,
    Intersted = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
    KeepAlive,
}

impl TryFrom<u8> for MsgType {
    type Error = Box<dyn Error + Send + Sync>;

    fn try_from(value: u8) -> Result<MsgType, Self::Error> {
        match value {
            0 => Ok(MsgType::Choke),
            1 => Ok(MsgType::Unchoke),
            2 => Ok(MsgType::Intersted),
            3 => Ok(MsgType::NotInterested),
            4 => Ok(MsgType::Have),
            5 => Ok(MsgType::Bitfield),
            6 => Ok(MsgType::Request),
            7 => Ok(MsgType::Piece),
            8 => Ok(MsgType::Cancel),
            // 9 => Ok(MsgType::KeepAlive),
            _ => Err("Invalid message type".into()),
        }
    }
}

impl Peer {
    pub async fn new(
        address: SocketAddrV4,
        info: Arc<MetaInfo>,
        tx: mpsc::Sender<PieceCommands>,
        ui_tx: mpsc::Sender<UiEvent>,
    ) -> Result<Peer, AsyncError> {
        let info_portion = &info.info;
        let raw_hash = to_vec(info_portion)?;
        let info_hash = sha1_hash(&raw_hash);

        let num_pieces = info.info.pieces.len().div_ceil(20);
        let socket = TcpStream::connect(address).await?;
        let mut left: u64 = 0;
        match &(info.info.mode) {
            FileMode::SingleFile { length } => left = *length,
            FileMode::MultipleFiles { files } => {
                for file in files {
                    left += file.length;
                }
            }
        }
        let peer_id = gen_peer_id();

        tx.send(PieceCommands::PeerRegister(peer_id, num_pieces))
            .await?;
        let _ = ui_tx
            .send(UiEvent::PeerUpdate {
                peer_id: String::from_utf8_lossy(&peer_id).to_string(),
                task: "Trying to connect".to_string(),
                choked: true,
            })
            .await;
        Ok(Peer {
            address,
            info: info.clone(),
            num_pieces,
            info_hash,
            socket,
            peer_id,
            sender: tx,
            total_size: left,
            bitfield: vec![false; num_pieces],
            outstanding: HashMap::new(),
            ui_tx,
        })
    }

    pub async fn start(&mut self) -> Result<(), AsyncError> {
        self.handshake().await?;

        let bitfield_future = self.read_until_type(MsgType::Bitfield);
        if let Ok(Ok((_, bitfield_payload))) =
            timeout(Duration::from_secs(TIMEOUT), bitfield_future).await
        {
            self.parse_bitfield(bitfield_payload);
            self.sender
                .send(PieceCommands::SetBitfield(
                    self.peer_id,
                    self.bitfield.clone(),
                ))
                .await?;
        }

        self.send_interested().await?;

        let unchoke_future = self.read_until_type(MsgType::Unchoke);
        let _ = timeout(Duration::from_secs(CHOKE_TIMEOUT), unchoke_future).await??;

        self.sender
            .send(PieceCommands::PeerUnchoke(self.peer_id))
            .await?;
        let _ = self
            .ui_tx
            .send(UiEvent::PeerUpdate {
                peer_id: String::from_utf8_lossy(&self.peer_id).to_string(),
                task: "Peer connection established".to_string(),
                choked: false,
            })
            .await;

        let mut first_try = true;
        loop {
            if self.outstanding.len() == 0 {
                let mut got_one = false;
                let mut req_str = String::from("Requesting index ");
                for _request in 0..REQUEST_ONCE {
                    let (oneshot_sender, oneshot_receiver) = oneshot::channel();
                    self.sender
                        .send(PieceCommands::RequestPieceIndex(
                            self.peer_id,
                            oneshot_sender,
                        ))
                        .await?;
                    if let Some(index) = oneshot_receiver.await? {
                        if self
                            .outstanding
                            .iter()
                            .find(|(k, _v)| **k == index)
                            .is_some()
                        {
                            break;
                        }
                        let piece_len = if index >= self.num_pieces - 1 {
                            self.total_size - index as u64 * self.info.info.piece_length
                        } else {
                            self.info.info.piece_length
                        };
                        self.outstanding.insert(
                            index,
                            PieceStatus {
                                data: vec![0u8; piece_len as usize],
                                blocks: vec![false; piece_len.div_ceil(BLOCK_LEN as u64) as usize],
                                received: 0,
                            },
                        );
                        let remaining = std::cmp::min(BLOCK_LEN, piece_len as u32);
                        self.send_request(index as u32, 0, remaining).await?;
                        req_str += &index.to_string();
                        req_str += ",";
                        got_one = true;
                    } else {
                        break;
                    }
                }
                if !got_one {
                    if !first_try {
                        break;
                    } else {
                        first_try = false;
                        sleep(Duration::from_secs(10)).await;
                    }
                }
                let _ = self
                    .ui_tx
                    .send(UiEvent::PeerUpdate {
                        peer_id: String::from_utf8_lossy(&self.peer_id).to_string(),
                        task: req_str.trim_end_matches(',').to_string(),
                        choked: false,
                    })
                    .await;
            } else {
                let (msg_type, payload) = self.read_message().await?;
                match msg_type {
                    MsgType::Choke => {
                        self.sender
                            .send(PieceCommands::PeerChoked(self.peer_id))
                            .await?;
                        for index in self.outstanding.keys() {
                            self.sender
                                .send(PieceCommands::PieceFailed(self.peer_id, *index))
                                .await?;
                        }
                        self.outstanding.clear();
                        timeout(
                            Duration::from_secs(CHOKE_TIMEOUT),
                            self.read_until_type(MsgType::Unchoke),
                        )
                        .await??;
                        self.sender
                            .send(PieceCommands::PeerUnchoke(self.peer_id))
                            .await?;
                        let _ = self
                            .ui_tx
                            .send(UiEvent::PeerUpdate {
                                peer_id: String::from_utf8_lossy(&self.peer_id).to_string(),
                                task: "Peer choked".to_string(),
                                choked: true,
                            })
                            .await;
                    }
                    MsgType::Unchoke => {
                        self.sender
                            .send(PieceCommands::PeerUnchoke(self.peer_id))
                            .await?;
                        let _ = self
                            .ui_tx
                            .send(UiEvent::PeerUpdate {
                                peer_id: String::from_utf8_lossy(&self.peer_id).to_string(),
                                task: "Peer unchoked".to_string(),
                                choked: false,
                            })
                            .await;
                    }
                    MsgType::Have => {
                        let index = u32::from_be_bytes(payload[0..4].try_into()?);
                        self.sender
                            .send(PieceCommands::UpdateBitfield(self.peer_id, index))
                            .await?;
                    }
                    MsgType::Request => {
                        self.send_choke().await?;
                    }
                    MsgType::Piece => {
                        let index = u32::from_be_bytes(payload[0..4].try_into().unwrap());
                        let begin = u32::from_be_bytes(payload[4..8].try_into().unwrap());
                        let block = payload[8..].to_vec();
                        let mut remove = false;
                        if let Some(piece) = self.outstanding.get_mut(&(index as usize)) {
                            let block_index = begin as usize / BLOCK_LEN as usize;
                            if block_index >= piece.blocks.len() {
                                continue;
                            }
                            if !piece.blocks[block_index] {
                                piece.blocks[block_index] = true;
                                piece.received += 1;
                            }

                            let start = begin as usize;
                            let end = start + block.len();
                            if end > piece.data.len() {
                                continue;
                            }
                            piece.data[start..end].copy_from_slice(&block);

                            if piece.received == piece.blocks.len()
                                && verify_hash(&piece.data, index as usize, &self.info.info.pieces)
                            {
                                // println!("Piece done");
                                remove = true;
                                write_piece_to_files(&self.info.info, index as usize, &piece.data)?;
                            } else {
                                if let Some((i, _)) =
                                    piece.blocks.iter().enumerate().find(|(_, b)| !**b)
                                {
                                    let begin = i as u32 * BLOCK_LEN;
                                    let remaining = piece.data.len() as u32 - begin;
                                    let req_len = remaining.min(BLOCK_LEN);
                                    let (oneshot_sender, oneshot_receiver) = oneshot::channel();
                                    self.sender
                                        .send(PieceCommands::RequestPieceStatus(
                                            index as usize,
                                            oneshot_sender,
                                        ))
                                        .await?;
                                    if let Some(piece_stat) = oneshot_receiver.await? {
                                        if piece_stat != PieceState::Done {
                                            let _ = self
                                                .ui_tx
                                                .send(UiEvent::PieceDownloading(index as usize))
                                                .await;
                                            self.send_request(index, begin, req_len).await?;
                                        } else {
                                            let _ =
                                                self.outstanding.remove_entry(&(index as usize));
                                            self.send_cancel(index, begin, req_len).await?;
                                        }
                                    } else {
                                        let _ = self.outstanding.remove_entry(&(index as usize));
                                        self.send_cancel(index, begin, req_len).await?;
                                    }
                                }
                            }
                        }
                        if remove {
                            let _ = self.outstanding.remove_entry(&(index as usize));
                            self.sender
                                .send(PieceCommands::PieceDone(self.peer_id, index as usize))
                                .await?;
                        }
                    }
                    _ => {}
                };
            }
        }

        Ok(())
    }

    async fn send_request(
        &mut self,
        piece_index: u32,
        begin: u32,
        block_len: u32,
    ) -> Result<(), AsyncError> {
        let mut msg = Vec::with_capacity(17);
        msg.extend(&(13u32.to_be_bytes())); // length
        msg.push(6u8); // message ID = request
        msg.extend(&piece_index.to_be_bytes());
        msg.extend(&begin.to_be_bytes());
        msg.extend(&block_len.to_be_bytes());

        self.socket.write_all(&msg).await?;
        Ok(())
    }

    async fn send_cancel(
        &mut self,
        piece_index: u32,
        begin: u32,
        block_len: u32,
    ) -> Result<(), AsyncError> {
        let mut msg = Vec::with_capacity(17);
        msg.extend(&(13u32.to_be_bytes())); // length
        msg.push(8u8); // message ID = cancel
        msg.extend(&piece_index.to_be_bytes());
        msg.extend(&begin.to_be_bytes());
        msg.extend(&block_len.to_be_bytes());

        self.socket.write_all(&msg).await?;
        Ok(())
    }

    async fn send_choke(&mut self) -> Result<(), AsyncError> {
        let mut buf = [0u8; 5];
        buf[0..4].copy_from_slice(&1u32.to_be_bytes());

        self.socket.write_all(&buf).await?;
        Ok(())
    }

    async fn send_interested(&mut self) -> Result<(), AsyncError> {
        let msg_len: [u8; 4] = [0, 0, 0, 1];
        let msg = 2;

        let mut packet = Vec::with_capacity(5);
        packet.extend_from_slice(&msg_len);
        packet.push(msg);

        self.socket.write_all(&packet).await?;

        Ok(())
    }

    fn parse_bitfield(&mut self, payload: Vec<u8>) {
        let mut index = 0;
        for byte in payload.iter() {
            for i in (0..8).rev() {
                let bit = (byte >> i) & 1;
                if self.num_pieces > index && bit == 1 {
                    self.bitfield[index] = true;
                }
                index += 1;
            }
        }
    }

    async fn read_until_type(
        &mut self,
        msg_type: MsgType,
    ) -> Result<(MsgType, Vec<u8>), AsyncError> {
        loop {
            match self.read_message().await {
                Ok((t, pl)) => {
                    if t == msg_type {
                        return Ok((t, pl));
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    async fn handshake(&mut self) -> Result<(), AsyncError> {
        let pstrlen: u8 = 19;
        let pstr = b"BitTorrent protocol";
        let reserved = [0u8; 8];

        let mut packet = Vec::with_capacity(68);
        packet.push(pstrlen);
        packet.extend_from_slice(pstr);
        packet.extend_from_slice(&reserved);
        packet.extend_from_slice(&self.info_hash);
        packet.extend_from_slice(&self.peer_id);

        self.socket.write_all(&packet).await?;

        let mut resp = [0u8; 68];

        if let Err(err) = self.socket.read_exact(&mut resp).await {
            return Err(format!("failed reading handshake: {err}").into());
        }

        if resp[0] != pstrlen {
            return Err("Handshake 1st byte did not match".into());
        } else if &resp[1..20] != pstr {
            return Err("BitTorrent protocol missing from resp".into());
        } else if &resp[28..48] != self.info_hash {
            return Err("Wrong info_hash returned from peer".into());
        }

        Ok(())
    }

    async fn read_message(&mut self) -> Result<(MsgType, Vec<u8>), AsyncError> {
        let mut len_buf = [0u8; 4];
        self.socket.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len == 0 {
            return Ok((MsgType::KeepAlive, vec![]));
        }

        let mut buf = vec![0u8; len as usize];
        self.socket.read_exact(&mut buf).await?;

        let msg_type = MsgType::try_from(buf[0])?;
        let payload = buf[1..].to_vec();

        Ok((msg_type, payload))
    }
}

fn verify_hash(piece_buf: &[u8], piece_index: usize, pieces_field: &[u8]) -> bool {
    let start = piece_index * 20;
    let end = start + 20;
    let expected_hash = &pieces_field[start..end];

    let mut hasher = Sha1::new();
    hasher.update(piece_buf);
    let result = hasher.finalize();

    result[..] == expected_hash[..]
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

                let f = std::fs::File::options()
                    .read(true)
                    .write(true)
                    .open(file_name)?;
                f.write_at(&remaining[..write_len], piece_offset)?;

                remaining = &remaining[write_len..];
                piece_offset = 0;

                if remaining.is_empty() {
                    break;
                }
            }
        }
        FileMode::SingleFile { length } => {
            let write_len = std::cmp::min(remaining.len() as u64, length - piece_offset) as usize;
            let f = std::fs::File::open(&info.name).unwrap();
            f.write_at(&remaining[..write_len], piece_offset)?;
            remaining = &remaining[write_len..];
        }
    }

    if !remaining.is_empty() {
        eprintln!("Warning: piece data exceeds file boundaries!");
    }

    Ok(())
}
