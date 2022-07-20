use std::future::Future;

use futures::future::BoxFuture;
use tokio::sync::{broadcast, mpsc};

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

pub struct TaskService {
    tokio_tasks: Vec<Box<dyn FnOnce(Shutdown, mpsc::Sender<()>) -> BoxFuture<'static, ()> + Send>>,
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
        F: FnOnce(Shutdown, mpsc::Sender<()>) -> BoxFuture<'static, ()> + Send + 'static,
    {
        self.tokio_tasks.push(Box::new(func));
    }

    pub async fn run(mut self, shutdown: impl Future) {
        log::info!("TaskService Start");
        tokio::select! {
            res = self.start() => {
                if let Err(err) = res {
                    log::error!("server failed: {}", err);
                }
            }
            _ = shutdown => {
                log::info!("server shutting down signal");
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
        for task in std::mem::replace(&mut self.tokio_tasks, vec![]) {
            handles.push(tokio::spawn(task(
                Shutdown::new(self.notify_shutdown.subscribe()),
                self.shutdown_complete_tx.clone(),
            )));
        }

        futures::future::join_all(handles).await;
        Ok(())
    }
}
