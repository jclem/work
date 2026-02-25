use std::sync::LazyLock;

use tokio::sync::{Notify, broadcast};

static SENDER: LazyLock<broadcast::Sender<()>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(64);
    tx
});

static SHUTDOWN: LazyLock<Notify> = LazyLock::new(Notify::new);

pub fn notify() {
    let _ = SENDER.send(());
}

pub fn subscribe() -> broadcast::Receiver<()> {
    SENDER.subscribe()
}

pub fn shutdown() {
    SHUTDOWN.notify_waiters();
}

pub async fn shutdown_notified() {
    SHUTDOWN.notified().await;
}
