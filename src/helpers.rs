/// Backoff calculator based off google's algorithm
pub fn exponential_backoff_ms(count: u64) -> u64 {
    use rand::Rng;

    const MAX_BACKOFF_MS: u64 = 32000;
    let mut rng = rand::thread_rng();
    let jitter_ms: u64 = rng.gen_range(0..1000);
    std::cmp::min(2u64.pow(count as u32) + jitter_ms, MAX_BACKOFF_MS)
}
