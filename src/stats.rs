// use lazy_static::lazy_static;
// use std::sync::Mutex;

// pub struct Stats {
//     pub fds: usize,
// }

pub fn get_fds() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

// lazy_static! {
//     pub static ref STATS: Mutex<Vec<Stats>> = Mutex::new(Vec::with_capacity(10240));
// }
