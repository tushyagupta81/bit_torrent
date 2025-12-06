use crate::peers::get_piece_from_peer;
use crate::peers::intitalize_peer_connections;
use crate::tracker::FileInfo;
use crate::tracker::get_peers;
use std::fs::{File, create_dir_all};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub fn download(torrent_file: String) {
    if let Ok(torrent) = get_peers(torrent_file) {
        let base_dir = Path::new("./download");
        initialize_files(base_dir, &torrent.files).unwrap();
        let mut pieces_done = vec![false; torrent.num_pieces];
        let mut peers = intitalize_peer_connections(&torrent.peers, &torrent.info_hash).unwrap();
        while pieces_done.iter().any(|&done| !done) {
            for peer in &mut peers {
                // Find the next missing piece that this peer actually has
                let piece_index_opt = pieces_done.iter().position(|&done| {
                    !done
                        && peer
                            .bitfield
                            .get(pieces_done.iter().position(|d| !d).unwrap_or(0))
                            .copied()
                            .unwrap_or(false)
                });

                if let Some(piece_index) = piece_index_opt {
                    match get_piece_from_peer(
                        piece_index,
                        torrent.piece_len as usize,
                        &torrent.pieces,
                        peer,
                    ) {
                        Ok(piece_buf) => {
                            write_piece_to_files(
                                base_dir,
                                &torrent.files,
                                piece_index,
                                torrent.piece_len as usize,
                                &piece_buf,
                            )
                            .unwrap();
                            pieces_done[piece_index] = true;
                            println!("Downloaded piece {}", piece_index);
                        }
                        Err(e) => {
                            println!(
                                "Error downloading piece {} from {}: {}",
                                piece_index, peer.ip, e
                            );
                        }
                    }
                }
            }

            // Optional: small sleep to avoid busy loop
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

fn write_piece_to_files(
    base_dir: &Path,
    files: &[FileInfo],
    piece_index: usize,
    piece_len: usize,
    piece_buf: &[u8],
) -> std::io::Result<()> {
    let mut piece_offset = piece_index as u64 * piece_len as u64;
    let mut remaining = piece_buf;

    for file in files {
        if piece_offset >= file.size as u64 {
            // Piece starts after this file, skip
            piece_offset -= file.size as u64;
            continue;
        }

        // How much of the piece fits into this file
        let write_len =
            std::cmp::min(remaining.len() as u64, file.size as u64 - piece_offset) as usize;

        let file_path = base_dir.join(&file.name);
        let mut f = File::options().write(true).open(&file_path)?;
        f.seek(SeekFrom::Start(piece_offset))?;
        f.write_all(&remaining[..write_len])?;

        remaining = &remaining[write_len..]; // Update remaining piece bytes
        piece_offset = 0; // Reset offset for next file

        if remaining.is_empty() {
            break;
        }
    }

    if !remaining.is_empty() {
        eprintln!("Warning: piece data exceeds file boundaries!");
    }

    Ok(())
}

fn initialize_files(base_dir: &Path, files: &[FileInfo]) -> std::io::Result<()> {
    // Ensure the base directory exists
    create_dir_all(base_dir)?;

    for file in files {
        let file_path = base_dir.join(&file.name);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            create_dir_all(parent)?;
        }

        let mut f = File::create(&file_path)?;
        if file.size > 0 {
            f.seek(SeekFrom::Start(file.size as u64 - 1))?;
            f.write_all(&[0])?; // allocate the space
        }
    }

    Ok(())
}
