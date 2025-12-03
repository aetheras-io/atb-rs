use std::future::Future;

use atb::helpers::exponential_backoff_ms;
use futures::future::BoxFuture;
use tokio::{
    sync::{broadcast, mpsc},
    time::{Duration, sleep},
};

pub const DEFAULT_MAX_BACKOFF_MS: u64 = 32000;

pub type ShutdownComplete = mpsc::Sender<()>;

#[derive(Debug)]
pub struct Shutdown {
    shutdown: bool,
    notify: broadcast::Receiver<()>,
}

impl Shutdown {
    pub fn new(notify: broadcast::Receiver<()>) -> Shutdown {
        Shutdown {
            shutdown: false,
            notify,
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub async fn recv(&mut self) {
        // If the shutdown signal has already been received, then return
        // immediately.
        if self.shutdown {
            return;
        }

        // Cannot receive a "lag error" as only one value is ever sent.
        let _ = self.notify.recv().await;

        // Remember that the signal has been received.
        self.shutdown = true;
    }
}

pub trait BoxFutureService:
    FnOnce(Shutdown, mpsc::Sender<()>) -> BoxFuture<'static, ()> + Send
{
}
impl<T: FnOnce(Shutdown, mpsc::Sender<()>) -> BoxFuture<'static, ()> + Send> BoxFutureService
    for T
{
}

pub struct TaskService {
    tokio_tasks: Vec<Box<dyn BoxFutureService>>,
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
    shutdown_complete_rx: Option<mpsc::Receiver<()>>,
}

impl TaskService {
    pub fn new() -> Self {
        let (notify_shutdown, _) = broadcast::channel(1);
        let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);

        Self {
            tokio_tasks: Vec::new(),
            notify_shutdown,
            shutdown_complete_tx,
            shutdown_complete_rx: Some(shutdown_complete_rx),
        }
    }

    /// add an `FnOnce` which produces a BoxFuture to be spawned as a tokio task.
    /// the task must listen on the `Shutdown` signals and drop the `mpsc::Sender<()>`
    /// when done.
    pub fn add_task<F>(&mut self, func: F)
    where
        F: BoxFutureService + 'static,
    {
        self.tokio_tasks.push(Box::new(func));
    }

    pub fn signals(&self) -> (Shutdown, ShutdownComplete) {
        (
            Shutdown::new(self.notify_shutdown.subscribe()),
            self.shutdown_complete_tx.clone(),
        )
    }

    pub fn take_completion_handle(&mut self) -> BoxFuture<'static, ()> {
        let mut rx = self
            .shutdown_complete_rx
            .take()
            .expect("completion_handle can only be taken once");

        Box::pin(async move {
            let _ = rx.recv().await;
            log::info!("TaskService shutdown completed");
        })
    }

    // Run will run until all tasks complete. If `take_completion_handle` is used,
    // the returned future can be awaited from a different context, but `run` will
    // still wait for each task to finish before returning.
    pub async fn run(self, shutdown: impl Future) {
        let Self {
            tokio_tasks,
            notify_shutdown,
            shutdown_complete_tx,
            shutdown_complete_rx,
        } = self;

        let mut handles = Vec::new();
        for task in tokio_tasks.into_iter() {
            handles.push(tokio::spawn(task(
                Shutdown::new(notify_shutdown.subscribe()),
                shutdown_complete_tx.clone(),
            )));
        }

        drop(shutdown_complete_tx);

        let join_all = futures::future::join_all(handles);
        tokio::pin!(join_all);

        let mut join_results = None;

        log::info!("TaskService started");
        let shutdown_triggered = tokio::select! {
            res = &mut join_all => {
                join_results = Some(res);
                false
            }
            _ = shutdown => {
                log::warn!("TaskService shutting down");
                true
            }
        };
        log::info!("TaskService stopped");

        if shutdown_triggered {
            if let Err(err) = notify_shutdown.send(()) {
                log::debug!("TaskService shutdown broadcast had no active listeners: {err}");
            }

            let res = join_all.await;
            join_results = Some(res);
        }

        if let Some(results) = join_results {
            for result in results {
                if let Err(err) = result {
                    if err.is_panic() {
                        log::error!("TaskService task panicked during shutdown: {err}");
                    } else if err.is_cancelled() {
                        log::warn!("TaskService task was cancelled during shutdown");
                    }
                }
            }
        }

        if let Some(mut rx) = shutdown_complete_rx {
            let _ = rx.recv().await;
            log::info!("TaskService shutdown completed");
        }

        // Dropping `notify_shutdown` after the explicit send ensures every subscriber sees the
        // shutdown signal, which allows each task to break its loop and drop its clone of
        // `shutdown_complete_tx`. Once all senders are gone, the completion receiver resolves.
    }
}

impl Default for TaskService {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn retry_future<Factory, Fut, F, O, E>(
    f: Factory,
    retryable: F,
    max_attempts: u64,
) -> Result<O, E>
where
    Factory: Fn() -> Fut,
    Fut: Future<Output = Result<O, E>>,
    F: Fn(&E) -> bool,
{
    let mut attempts = 0;

    loop {
        match f().await {
            Ok(o) => return Ok(o),
            Err(e) => {
                if attempts < max_attempts && retryable(&e) {
                    attempts += 1;
                    let backoff = exponential_backoff_ms(attempts, DEFAULT_MAX_BACKOFF_MS);
                    sleep(Duration::from_millis(backoff)).await;
                    continue;
                }

                return Err(e);
            }
        }
    }
}

pub mod stream {
    use super::*;

    use async_trait::async_trait;
    use futures::{FutureExt, Sink, Stream, StreamExt};

    #[async_trait]
    pub trait StreamProcessor: Send + 'static {
        type Stream: Stream<Item = Result<Self::Item, Self::Error>>
            + Sink<Self::Item>
            + Send
            + 'static;
        type Item: std::fmt::Debug + Send + 'static;
        type Error: std::error::Error + Send + 'static;

        fn identifier(&self) -> &str;
        async fn connect(&mut self) -> anyhow::Result<Self::Stream>;
        async fn process(&mut self, message: Self::Item) -> anyhow::Result<()>;
    }

    pub struct StreamReactor<T> {
        pub processor: T,
    }

    impl<T> StreamReactor<T>
    where
        T: StreamProcessor,
    {
        pub fn to_boxed_task_fn(
            self,
        ) -> impl FnOnce(Shutdown, ShutdownComplete) -> BoxFuture<'static, ()> {
            move |shutdown, shutdown_complete| self.run(shutdown, shutdown_complete).boxed()
        }

        pub async fn run(mut self, mut shutdown: Shutdown, _shutdown_complete: ShutdownComplete) {
            while !shutdown.is_shutdown() {
                tokio::select! {
                    _ = async {
                        let mut connection_attempts = 0;
                        loop {
                            log::info!("StreamReactor connecting...");
                            let stream = match self.processor.connect().await {
                                Ok(s) => s,
                                Err(e) => {
                                    let backoff = exponential_backoff_ms(
                                        connection_attempts,
                                        DEFAULT_MAX_BACKOFF_MS
                                    );
                                    log::info!(
                                        "StreamReactor[{}] connect error: {}, attempts: {}, retry in {}ms",
                                        self.processor.identifier(),
                                        e,
                                        connection_attempts,
                                        backoff
                                    );
                                    sleep(Duration::from_millis(backoff)).await;
                                    connection_attempts += 1;
                                    continue;
                                }
                            };
                            connection_attempts = 0;

                            let (_write, mut read) = stream.split();

                            while let Some(Ok(msg)) = read.next().await {
                                if let Err(e) = self.processor.process(msg).await {
                                    log::info!("StreamReactor processor error: {}, restarting", e);
                                    break;
                                }
                            }
                        }
                    } => {
                        log::info!("StreamReactor processor stopped?");
                    },

                    _ = shutdown.recv() => {
                        log::info!("StreamReactor shutting down from signal");
                        break;
                    }
                }
            }
            log::info!("StreamReactor stopped")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::StreamProcessor;
    use anyhow::anyhow;
    use async_trait::async_trait;
    use futures::{Sink, Stream};
    use std::{
        collections::VecDeque,
        fmt,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };
    use tokio::{
        sync::{Notify, broadcast, mpsc, oneshot},
        time::{Duration, Instant, sleep, timeout},
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestError {
        attempt: usize,
        kind: &'static str,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} attempt {}", self.kind, self.attempt)
        }
    }

    impl std::error::Error for TestError {}

    #[derive(Debug, Clone)]
    struct TestStreamError;

    impl fmt::Display for TestStreamError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "test stream error")
        }
    }

    impl std::error::Error for TestStreamError {}

    struct TestStream {
        items: VecDeque<Result<u8, TestStreamError>>,
    }

    impl TestStream {
        fn new(items: Vec<Result<u8, TestStreamError>>) -> Self {
            Self {
                items: VecDeque::from(items),
            }
        }
    }

    impl Stream for TestStream {
        type Item = Result<u8, TestStreamError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            Poll::Ready(this.items.pop_front())
        }
    }

    impl Sink<u8> for TestStream {
        type Error = TestStreamError;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, _item: u8) -> Result<(), Self::Error> {
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    struct TestProcessor {
        id: &'static str,
        streams: Arc<Mutex<VecDeque<Vec<Result<u8, TestStreamError>>>>>,
        process_results: Arc<Mutex<VecDeque<anyhow::Result<()>>>>,
        connect_calls: Arc<AtomicUsize>,
        processed: Arc<Mutex<Vec<u8>>>,
        processed_notify: Arc<Notify>,
    }

    impl TestProcessor {
        fn new(
            id: &'static str,
            streams: Vec<Vec<Result<u8, TestStreamError>>>,
            process_results: Vec<anyhow::Result<()>>,
            processed_notify: Arc<Notify>,
        ) -> Self {
            Self {
                id,
                streams: Arc::new(Mutex::new(VecDeque::from(streams))),
                process_results: Arc::new(Mutex::new(VecDeque::from(process_results))),
                connect_calls: Arc::new(AtomicUsize::new(0)),
                processed: Arc::new(Mutex::new(Vec::new())),
                processed_notify,
            }
        }
    }

    #[async_trait]
    impl StreamProcessor for TestProcessor {
        type Stream = TestStream;
        type Item = u8;
        type Error = TestStreamError;

        fn identifier(&self) -> &str {
            self.id
        }

        async fn connect(&mut self) -> anyhow::Result<Self::Stream> {
            let maybe_stream = self.streams.lock().unwrap().pop_front();
            match maybe_stream {
                Some(items) => {
                    self.connect_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(TestStream::new(items))
                }
                None => Err(anyhow!("no more streams")),
            }
        }

        async fn process(&mut self, message: Self::Item) -> anyhow::Result<()> {
            self.processed.lock().unwrap().push(message);
            self.processed_notify.notify_one();

            if let Some(result) = self.process_results.lock().unwrap().pop_front() {
                result
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tasks_receive_shutdown_and_complete_gracefully() {
        let mut service = TaskService::new();

        let (task_ready_tx, task_ready_rx) = oneshot::channel();
        let (shutdown_seen_tx, shutdown_seen_rx) = oneshot::channel();

        service.add_task(move |mut shutdown, shutdown_complete| {
            Box::pin(async move {
                task_ready_tx.send(()).ok();
                shutdown.recv().await;
                shutdown_seen_tx.send(()).ok();
                drop(shutdown_complete);
            })
        });

        let completion = service.take_completion_handle();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let join_handle = tokio::spawn(service.run(async {
            let _ = shutdown_rx.await;
        }));

        timeout(Duration::from_secs(1), task_ready_rx)
            .await
            .expect("task did not become ready")
            .expect("task readiness channel closed unexpectedly");

        shutdown_tx
            .send(())
            .expect("failed to trigger service shutdown");

        timeout(Duration::from_secs(1), join_handle)
            .await
            .expect("service::run did not finish in time")
            .expect("service::run join handle returned error");

        timeout(Duration::from_secs(1), shutdown_seen_rx)
            .await
            .expect("task never observed shutdown signal")
            .expect("shutdown observer channel closed before notification");

        timeout(Duration::from_secs(1), completion)
            .await
            .expect("service completion handle did not resolve");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_waits_for_tasks_to_finish_after_shutdown() {
        let mut service = TaskService::new();

        service.add_task(|mut shutdown, shutdown_complete| {
            Box::pin(async move {
                shutdown.recv().await;
                // simulate cleanup work that must finish before shutdown completes
                sleep(Duration::from_millis(50)).await;
                drop(shutdown_complete);
            })
        });

        let completion = service.take_completion_handle();
        drop(completion);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let run_handle = tokio::spawn(service.run(async {
            let _ = shutdown_rx.await;
        }));

        // give the task a moment to start
        tokio::task::yield_now().await;

        let start = Instant::now();
        shutdown_tx
            .send(())
            .expect("failed to trigger service shutdown");

        timeout(Duration::from_secs(1), run_handle)
            .await
            .expect("service::run did not finish in time")
            .expect("service::run join handle returned error");

        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(50),
            "service::run returned before tasks completed cleanup: {elapsed:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_future_succeeds_without_retry() {
        let result = retry_future(
            || async { Ok::<_, TestError>(123) },
            |_err: &TestError| true,
            3,
        )
        .await;

        assert_eq!(result.unwrap(), 123);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_future_retries_then_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = attempts.clone();

        let result = retry_future(
            move || {
                let attempts_for_factory = attempts_for_factory.clone();
                async move {
                    let attempt = attempts_for_factory.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt < 3 {
                        Err(TestError {
                            attempt,
                            kind: "retryable",
                        })
                    } else {
                        Ok::<_, TestError>("done")
                    }
                }
            },
            |err: &TestError| err.kind == "retryable",
            5,
        )
        .await
        .unwrap();

        assert_eq!(result, "done");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_future_stops_on_non_retryable_error() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = attempts.clone();

        let result = retry_future(
            move || {
                let attempts_for_factory = attempts_for_factory.clone();
                async move {
                    let attempt = attempts_for_factory.fetch_add(1, Ordering::SeqCst) + 1;
                    Err::<(), TestError>(TestError {
                        attempt,
                        kind: "fatal",
                    })
                }
            },
            |err: &TestError| err.kind == "retryable",
            5,
        )
        .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let err = result.unwrap_err();
        assert_eq!(err.kind, "fatal");
        assert_eq!(err.attempt, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retry_future_respects_max_attempts() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = attempts.clone();

        let err = retry_future(
            move || {
                let attempts_for_factory = attempts_for_factory.clone();
                async move {
                    let attempt = attempts_for_factory.fetch_add(1, Ordering::SeqCst) + 1;
                    Err::<(), TestError>(TestError {
                        attempt,
                        kind: "retryable",
                    })
                }
            },
            |err: &TestError| err.kind == "retryable",
            2,
        )
        .await
        .unwrap_err();

        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(err.kind, "retryable");
        assert_eq!(err.attempt, 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stream_reactor_reconnects_after_processor_error() {
        let notify = Arc::new(Notify::new());

        let processor = TestProcessor::new(
            "test",
            vec![vec![Ok(1)], vec![Ok(2)]],
            vec![Err(anyhow!("boom")), Ok(())],
            notify.clone(),
        );

        let connect_calls = processor.connect_calls.clone();
        let processed = processor.processed.clone();

        let reactor = stream::StreamReactor { processor };

        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown = Shutdown::new(shutdown_tx.subscribe());
        let (complete_tx, _) = mpsc::channel(1);

        let handle = tokio::spawn(async move {
            reactor.run(shutdown, complete_tx).await;
        });

        notify.notified().await;
        assert!(
            connect_calls.load(Ordering::SeqCst) >= 1,
            "expected at least one connection attempt"
        );

        notify.notified().await;
        assert!(
            connect_calls.load(Ordering::SeqCst) >= 2,
            "expected reconnection after processor error"
        );
        assert_eq!(*processed.lock().unwrap(), vec![1, 2]);

        shutdown_tx
            .send(())
            .expect("failed to send shutdown signal for stream reactor");

        timeout(Duration::from_secs(1), handle)
            .await
            .expect("stream reactor did not stop in time")
            .expect("stream reactor task panicked");
    }
}
