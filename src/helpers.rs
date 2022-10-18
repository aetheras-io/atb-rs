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

pub trait InspectError {
    fn inspect_err(self, message: &str) -> Self;
}

impl<T, E: std::error::Error + 'static> InspectError for Result<T, E> {
    fn inspect_err(self, message: &str) -> Self {
        if let Err(e) = &self {
            log::error!("{}: {:?}", message, e);
        }
        self
    }
}

pub trait Inspect {
    fn inspect(self, message: &str) -> Self;
}

impl<T: std::fmt::Debug, E> Inspect for Result<T, E> {
    fn inspect(self, message: &str) -> Self {
        if let Ok(r) = &self {
            log::info!("{}: {:?}", message, r);
        }
        self
    }
}
