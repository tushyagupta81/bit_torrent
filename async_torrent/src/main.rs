mod app;
mod bencode;
mod engine;
mod tui;
mod utils;

use anyhow;
use std::env;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

fn redirect_stderr() {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("debug.log")
        .unwrap();

    unsafe {
        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_torrent>", args[0]);
        std::process::exit(1);
    }
    let torrent_path: PathBuf = PathBuf::from(&args[1]);
    redirect_stderr();
    let info = bencode::decode_bencode(torrent_path).unwrap();
    app::run_tui(info).await
}
