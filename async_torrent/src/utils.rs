use rand::Rng;
use sha1::{Digest, Sha1};

pub fn gen_peer_id() -> [u8; 20] {
    let mut id = *b"-RS0001-000000000000";
    let mut rng = rand::rng();
    for i in id.iter_mut().skip(8) {
        *i = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
            [rng.random_range(0..62)];
    }
    id
}

pub fn sha1_hash(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let hash = hasher.finalize().into();
    hash
}

pub fn encode_binary(data: &[u8]) -> String {
    use url::form_urlencoded::byte_serialize;
    byte_serialize(data).collect()
}
