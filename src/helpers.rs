/// Backoff calculator based off google's algorithm
/// #NOTE, jitter is randomized in the range of 0-1000 ms
/// `max_backoff_ms` is suggested to be 32000ms to 64000ms by google
pub fn exponential_backoff_ms(count: u64, max_backoff_ms: u64) -> u64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    std::cmp::min(
        2u64.pow(count as u32) + rng.gen_range(0..1000),
        max_backoff_ms,
    )
}
