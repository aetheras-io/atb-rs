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

    // Run will run until all tasks complete.  If the shutdown_complete_handle is taken, it is the
    // calling scope's responsibility to wait until all tasks complete.  For example:
    // ```rs
    // fn main() {
    //  //setup
    //  let services_complete = service.take_completion_handle();
    //  runtime.spawn(service.run(tokio::signal::ctrl_c()));
    //  // ..other stuff
    //  runtime.block_on(services_complete);
    // }
    //
    // ```
    // Runtime.block_on(service.run_to_completion()) is the last call
    pub async fn run(self, shutdown: impl Future) {
        let Self {
            tokio_tasks,
            notify_shutdown,
            shutdown_complete_tx,
            shutdown_complete_rx,
        } = self;

        let start = async move {
            let mut handles = Vec::new();
            for task in tokio_tasks.into_iter() {
                handles.push(tokio::spawn(task(
                    Shutdown::new(notify_shutdown.subscribe()),
                    shutdown_complete_tx.clone(),
                )));
            }

            futures::future::join_all(handles).await;
        };

        log::info!("TaskService started");
        tokio::select! {
            _ = start => {}
            _ = shutdown => {
                log::warn!("TaskService shutting down");
            }
        }
        log::info!("TaskService stopped");

        if let Some(mut rx) = shutdown_complete_rx {
            let _ = rx.recv().await;
            log::info!("TaskService shutdown completed");
        }

        // #NOTE `self.notify_shutdown` is now dropped, `notify_shutdown` will now fire to all
        // spawned tasks indefinitely.  This in turn should break the task loops and drop the
        // clones of `shutdown_complete_tx`.  Now the outer thread / caller will be waiting
        // gracefully on the `shutdown_complete_rx` for all tasks to finish
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
        // drop(shutdown_complete_rx);
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
