use std::{
    fs::{File, create_dir_all},
    path::Path,
};

use crate::bencode::{FileMode, Info};

pub fn initialize_files(info: &Info) -> std::io::Result<()> {
    match &info.mode {
        FileMode::MultipleFiles { files } => {
            for file in files {
                // Open or create the file
                let file_name = format!("{}/{}", info.name, file.path.join("/"));

                if let Some(parent) = Path::new(file_name.as_str()).parent() {
                    create_dir_all(parent)?;
                }

                let f = File::options()
                    .create(true)
                    .read(true)
                    .write(true)
                    .truncate(true)
                    .open(file_name)?;
                f.set_len(file.length)?; // preallocate
            }
        }
        FileMode::SingleFile { length } => {
            let file_name = info.name.as_str();

            if let Some(parent) = Path::new(file_name).parent() {
                create_dir_all(parent)?;
            }

            let f = File::options()
                .create(true)
                .read(true)
                .write(true)
                .truncate(true)
                .open(file_name)?;
            f.set_len(*length)?; // preallocate
        }
    };

    Ok(())
}
