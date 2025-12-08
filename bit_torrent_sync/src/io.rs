use std::io::{Seek, SeekFrom, Write};

use crate::download::FileHandle;

pub fn write_piece_to_files(
    files: &[FileHandle],
    piece_index: usize,
    piece_len: usize,
    piece_buf: &[u8],
) -> std::io::Result<()> {
    let mut piece_offset = piece_index as u64 * piece_len as u64;
    let mut remaining = piece_buf;

    for file in files {
        if piece_offset >= file.size {
            piece_offset -= file.size;
            continue;
        }

        let write_len = std::cmp::min(remaining.len() as u64, file.size - piece_offset) as usize;

        {
            let mut f = file.file.lock().unwrap();
            f.seek(SeekFrom::Start(piece_offset))?;
            f.write_all(&remaining[..write_len])?;
        }

        remaining = &remaining[write_len..];
        piece_offset = 0;

        if remaining.is_empty() {
            break;
        }
    }

    if !remaining.is_empty() {
        eprintln!("Warning: piece data exceeds file boundaries!");
    }

    Ok(())
}
