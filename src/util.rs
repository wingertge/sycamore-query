use fnv::FnvHasher;
use std::hash::{Hash, Hasher};

pub fn hash_key<K: Hash>(k: K) -> u64 {
    let mut hash = FnvHasher::default();
    k.hash(&mut hash);
    hash.finish()
}
