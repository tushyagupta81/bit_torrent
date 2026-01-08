use std::{error::Error, time::Duration};

use serde::Deserialize;
use serde_bencoded::{from_bytes, to_vec};
use tokio::time::timeout;

use crate::{
    bencode::{FileMode, MetaInfo},
    engine::network,
    engine::peers::Peers,
    utils::{encode_binary, gen_peer_id, sha1_hash},
};

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    pub interval: usize,
    pub peers: Peers,
}

pub async fn fetch_peers(info: &MetaInfo) -> Result<TrackerResponse, Box<dyn Error + Send + Sync>> {
    let info_portion = &info.info;
    let raw_hash = to_vec(info_portion)?;
    let info_hash = sha1_hash(&raw_hash);

    let mut trackers = vec![];
    if let Some(t) = &(info.announce_list) {
        for tracker in t {
            trackers.push(tracker[0].clone());
        }
    } else {
        trackers.push(info.announce.clone());
    }

    let peer_id = gen_peer_id();
    let port = 6881;
    let downloaded: u64 = 0;
    let uploaded: u64 = 0;
    let mut left: u64 = 0;
    match &(info.info.mode) {
        FileMode::SingleFile { length } => left = *length,
        FileMode::MultipleFiles { files } => {
            for file in files {
                left += file.length;
            }
        }
    }
    let compact = 1;
    let event = "started";
    let numwant: u32 = 50;

    let timeout_dur = 5;

    for tracker in trackers {
        if tracker.starts_with("http") {
            let query = format!(
                "info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact={}&event={}&numwant={}",
                encode_binary(&info_hash),
                encode_binary(&peer_id),
                port,
                uploaded,
                downloaded,
                left,
                compact,
                event,
                numwant
            );
            let sep = if tracker.contains("?") { "&" } else { "?" };
            let full_url = format!("{}{}{}", tracker, sep, query,);
            let peers_bytes = match timeout(
                Duration::from_secs(timeout_dur),
                network::http::get_peers(full_url),
            )
            .await
            {
                Ok(r) => match r {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to get peer(HTTP): {e}");
                        continue;
                    }
                },
                Err(e) => {
                    eprintln!("Failed to get peer(HTTP): {e}");
                    continue;
                }
            };
            let peers: TrackerResponse = match from_bytes(&peers_bytes) {
                Ok(tr) => tr,
                Err(e) => {
                    eprintln!("Error: {e}, in {}", String::from_utf8_lossy(&peers_bytes));
                    continue;
                }
            };
            return Ok(peers);
        } else if tracker.starts_with("udp") {
            let p = match timeout(
                Duration::from_secs(timeout_dur),
                network::udp::get_peers(
                    tracker, &info_hash, &peer_id, left, port, downloaded, uploaded, numwant,
                ),
            )
            .await
            {
                Ok(r) => match r {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to get peer(HTTP): {e}");
                        continue;
                    }
                },
                Err(e) => {
                    eprintln!("Failed to get peer(HTTP): {e}");
                    continue;
                }
            };
            let peers = TrackerResponse {
                interval: 0,
                peers: p,
            };
            return Ok(peers);
        }
    }

    Err("Failed to fetch peers".into())
}
