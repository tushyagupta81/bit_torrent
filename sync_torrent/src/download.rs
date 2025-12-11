use crate::peers::intitalize_peer_connections;
use crate::tracker::FileInfo;
use crate::tracker::get_peers;
use std::fs::{File, create_dir_all};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
#[allow(dead_code)]
pub struct FileHandle {
    pub path: PathBuf,
    pub size: u64,
    pub file: Arc<Mutex<File>>,
}

pub fn download(torrent_file: String) {
    if let Ok(torrent) = get_peers(torrent_file) {
        let base_dir = Path::new("./download");
        let file_handles = match initialize_files(base_dir, &torrent.files) {
            Ok(f) => f,
            Err(e) => {
                panic!("Failed to initialize_files: {e}");
            }
        };
        match intitalize_peer_connections(&torrent, &file_handles) {
            Ok(_) => {}
            Err(e) => {
                println!("Error while downloading: {e}");
            }
        };
    }
}

fn initialize_files(base_dir: &Path, files: &[FileInfo]) -> std::io::Result<Vec<FileHandle>> {
    let mut file_handles = Vec::new();
    for f in files {
        let full_path = base_dir.join(&f.name);

        // Ensure the parent directory exists
        if let Some(parent) = full_path.parent() {
            create_dir_all(parent)?;
        }

        // Open or create the file
        let file = File::options()
            .create(true) // create if missing
            .truncate(true) // optional: clear existing contents
            .read(true)
            .write(true)
            .open(&full_path)?;

        file.set_len(f.size)?; // preallocate

        file_handles.push(FileHandle {
            path: full_path,
            size: f.size,
            file: Arc::new(Mutex::new(file)),
        });
    }
    Ok(file_handles)
}
