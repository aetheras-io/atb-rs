/// Backoff calculator based off google's algorithm
/// #NOTE, jitter is randomized in the range of 0-1000 ms
pub fn exponential_backoff_ms(count: u64, max_backoff_ms: Option<u64>) -> u64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    std::cmp::min(
        2u64.pow(count as u32) + rng.gen_range(0..1000),
        max_backoff_ms.unwrap_or(32000),
    )
}
