use std::time::{Duration, Instant};

fn main() {
    for ms in [1, 5, 10, 50, 100, 200] {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(ms));
        let elapsed = start.elapsed();
        println!("sleep({}ms) took {:?} ({:.1}x)", ms, elapsed,
            elapsed.as_secs_f64() * 1000.0 / ms as f64);
    }
}
