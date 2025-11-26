use core::panic;
use std::fs::{self};

// d - e -> dictionary pair
// l - e -> list pair
// i - e -> interger pair

#[derive(Debug, Clone)]
struct FileInfo {
    length: u32,
    path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Info {
    announce: Option<String>,
    announce_list: Vec<String>,
    comment: Option<String>,
    created_by: Option<String>,
    creation_date: Option<String>,
    encoding: Option<String>,
    info: Vec<FileInfo>,
    name: Option<String>,
    piece_length: Option<u32>,
    pieces: Vec<u8>,
    url_list: Option<String>,
}

pub fn decode(file_path: String) -> Info {
    let mut dict = Info {
        announce: None,
        announce_list: Vec::new(),
        comment: None,
        created_by: None,
        creation_date: None,
        encoding: None,
        info: Vec::new(),
        name: None,
        piece_length: None,
        pieces: Vec::new(),
        url_list: None,
    };

    let file = match fs::read(file_path) {
        Ok(file) => file,
        Err(e) => panic!("Failed to open file: {e:?}"),
    };
    let file_len = file.len();

    let mut start: usize = 0;
    let mut i: usize = 0;
    let mut prev: Vec<&str> = Vec::new();
    while i < file_len {
        let &byte = &file[i];
        if byte.is_ascii_digit() {
            start = i;
            while i < file_len && file[i].is_ascii_digit() {
                i += 1;
            }
            i -= 1;
        } else if byte == b'i' {
            if !prev.is_empty() {
                let &prev_field = prev.last().unwrap();
                match prev_field {
                    "length" => {
                        start = i + 1;
                        while file[i] != b'e' {
                            i += 1;
                        }
                        i -= 1;
                        if start >= i {
                            i = start;
                            continue;
                        }
                        let field = str::from_utf8(&file[start..i]).unwrap();
                        dict.info.push(FileInfo {
                            length: match field.to_string().parse::<u32>() {
                                Ok(l) => l,
                                Err(e) => panic!("Unable to parse '{field}': {e:?}"),
                            },
                            path: None,
                        });
                    }
                    "creation date" => {
                        start = i + 1;
                        while file[i] != b'e' {
                            i += 1;
                        }
                        i -= 1;
                        if start >= i {
                            i = start;
                            continue;
                        }
                        let field = str::from_utf8(&file[start..i]).unwrap();
                        dict.creation_date = Some(field.to_string());
                    }
                    "piece length" => {
                        start = i + 1;
                        while file[i] != b'e' {
                            i += 1;
                        }
                        i -= 1;
                        if start >= i {
                            i = start;
                            continue;
                        }
                        let field = str::from_utf8(&file[start..i]).unwrap();
                        dict.piece_length = match field.to_string().parse::<u32>() {
                            Ok(pl) => Some(pl),
                            Err(e) => panic!("Unable to parse '{field}': {e:?}"),
                        }
                    }
                    _ => (),
                }
            }
        } else if byte == b'e' {
            start = i;
            prev.pop();
        } else if byte == b':' {
            let content_len = match str::from_utf8(&file[start..i]) {
                Ok(f) => match f.to_string().parse::<usize>() {
                    Ok(len) => len,
                    Err(e) => panic!("Error '{f}': {e:?}"),
                },
                Err(e) => panic!("Unable to parse: {e:?}"),
            };
            let field = match str::from_utf8(&file[i + 1..i + 1 + content_len as usize]) {
                Ok(f) => f,
                Err(e) => {
                    println!("Unable to parse {e:?}");
                    ""
                }
            };
            if !prev.is_empty() {
                let &prev_field = prev.last().unwrap();
                match prev_field {
                    "announce" => dict.announce = Some(field.to_string()),
                    "announce-list" => dict.announce_list.push(field.to_string()),
                    "comment" => dict.comment = Some(field.to_string()),
                    "created by" => dict.created_by = Some(field.to_string()),
                    "creation date" => dict.creation_date = Some(field.to_string()),
                    "encoding" => dict.encoding = Some(field.to_string()),
                    "path" => dict.info.last_mut().unwrap().path = Some(field.to_string()),
                    "name" => dict.name = Some(field.to_string()),
                    "url-list" => dict.url_list = Some(field.to_string()),
                    "pieces" => {
                        for &b in &file[i + 1..i + 1 + content_len] {
                            dict.pieces.push(b);
                        }
                    }
                    _ => (),
                }
            }
            prev.push(field);
            i += content_len;
            start = i;
        }
        i += 1;
    }
    dbg!(dict.clone());

    return dict;
}
