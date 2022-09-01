use std::future::Future;

use atb::helpers::exponential_backoff_ms;
use futures::future::BoxFuture;
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, Duration},
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
}

impl TaskService {
    pub fn new() -> (Self, mpsc::Receiver<()>) {
        let (notify_shutdown, _) = broadcast::channel(1);
        let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);

        (
            Self {
                tokio_tasks: Vec::new(),
                notify_shutdown,
                shutdown_complete_tx,
            },
            shutdown_complete_rx,
        )
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

    pub async fn run(mut self, shutdown: impl Future) {
        log::info!("TaskService Start");
        tokio::select! {
            res = self.start() => {
                if let Err(err) = res {
                    log::error!("TaskService failed: {}", err);
                }
            }
            _ = shutdown => {
                log::warn!("TaskService shutting down");
            }
        }
        log::info!("TaskService stopped");

        // #NOTE `self.notify_shutdown` is now dropped, `notify_shutdown` will now fire to all
        // spawned tasks indefinitely.  This in turn should break the task loops and drop the
        // clones of `shutdown_complete_tx`.  Now the outer thread / caller will be waiting on
        // the `shutdown_complete_rx`, waiting gracefully for all tasks to finish
        //
        // The following "signaling" happens:
        // let TaskService {
        //     tokio_tasks,
        //     notify_shutdown,
        //     shutdown_complete_tx,
        // } = self;
        // drop(tokio_tasks);
        // drop(notify_shutdown);
        // drop(shutdown_complete_tx);
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        let mut handles = Vec::new();
        for task in std::mem::take(&mut self.tokio_tasks) {
            handles.push(tokio::spawn(task(
                Shutdown::new(self.notify_shutdown.subscribe()),
                self.shutdown_complete_tx.clone(),
            )));
        }

        futures::future::join_all(handles).await;
        Ok(())
    }
}

pub async fn retry_future<Factory, Fut, F, O, E>(f: Factory, retryable: F, max_attempts: u64) -> O
where
    Factory: Fn() -> Fut,
    Fut: Future<Output = Result<O, E>>,
    F: Fn(&E) -> bool,
{
    let mut attempts = 0;
    loop {
        match f().await {
            Ok(o) => return o,
            Err(e) => {
                if attempts <= max_attempts && retryable(&e) {
                    attempts += 1;
                    let backoff = exponential_backoff_ms(attempts, DEFAULT_MAX_BACKOFF_MS);
                    sleep(Duration::from_millis(backoff)).await;
                    continue;
                }
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
