use serde::{Deserialize, Serialize};
use serde_bencoded::from_bytes;
use serde_bytes::ByteBuf;

pub fn decode_bencode(path: String) -> Result<MetaInfo, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(&path).unwrap();
    let info: MetaInfo = from_bytes(&bytes)?;
    // println!("announce: {}", info.announce);
    // println!("announce-list: {:?}", info.announce_list);
    // println!("creation date: {:?}", info.creation_date);
    // println!("comment: {:?}", info.comment);
    // println!("created by: {:?}", info.created_by);
    // println!("encoding: {:?}", info.encoding);
    //
    // println!(
    //     "piece length: {}",
    //     bytesize::ByteSize(info.info.piece_length).to_string()
    // );
    // println!("pieces (count): {}", info.info.pieces.len() as u64);
    // println!("private: {:?}", info.info.private);
    // println!("name: {}", info.info.name);
    // match info.info.mode {
    //     // display as Single File Mode
    //     FileMode::SingleFile { length, md5sum } => {
    //         println!("\tSingle File Mode");
    //         println!("\tlength: {}", bytesize::ByteSize(length));
    //         println!("\tmd5sum: {:?}", md5sum);
    //     }
    //     // display as Multiple File Mode
    //     FileMode::MultipleFiles { files } => {
    //         println!("\tMultiple File Mode");
    //         println!("\tfiles:");
    //         for file in files {
    //             println!("\t\tlength: {}", file.length);
    //             println!("\t\tmd5sum: {:?}", file.md5sum);
    //             println!("\t\tpath: {:?}", file.path);
    //             println!();
    //         }
    //     }
    // }

    Ok(info)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    pub pieces: ByteBuf,
    pub name: String,
    #[serde(flatten)]
    pub mode: FileMode,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileMode {
    MultipleFiles {
        files: Vec<File>,
    },
    SingleFile {
        length: u64,
        // md5sum: Option<ByteBuf>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct File {
    pub length: u64,
    // pub md5sum: Option<ByteBuf>,
    pub path: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaInfo {
    pub info: Info,
    pub announce: String,
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    #[serde(rename = "creation date")]
    pub creation_date: Option<u64>,
    pub comment: Option<String>,
    #[serde(rename = "created by")]
    pub created_by: Option<String>,
    pub encoding: Option<String>,
}
