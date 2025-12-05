use crate::tracker::get_peers;
use crate::peers::get_piece_from_peer;

fn download(torrent_file: String) {
    if let Ok((peers, hash, piece_len, num_pieces)) = get_peers(torrent_file) {
        let pieces_done = vec![false;num_pieces];
        for peer in peers {
            let mut piece_index: usize = 0;
            while piece_index < num_pieces {
                if pieces_done[piece_index] {
                    continue;
                }
                get_piece_from_peer(peer.ip.clone(), peer.port, &hash, piece_len, piece_index);
                piece_index+=1;
            }
        }
    }
}
