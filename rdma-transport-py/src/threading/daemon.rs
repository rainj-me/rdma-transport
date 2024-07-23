use oneshot::{Receiver, Sender};
use pyo3::prelude::*;
use tokio::runtime::Runtime;
use std::thread;
use std::time::Duration;


#[pyclass]
pub struct DaemonThread {
    handle: Option<thread::JoinHandle<()>>,
    sender: Option<Sender<u8>>
}

pub async fn sleep_forever() {
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

pub async fn stop(rx: Receiver<u8>) -> u8 {
    if let Ok(res) = rx.recv() {
        return res;
    }
    0
}

#[pymethods]
impl DaemonThread {
    #[new]
    fn new() -> Self {
        let (tx, rx) = oneshot::channel::<u8>();
        let handle = thread::spawn(|| {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    tokio::select! {
                        _ = stop(rx) => {
                            break;
                        }
                        _ = sleep_forever() => {
                            println!("sleep forever");
                        }
                    }
                }
            });
        });

        DaemonThread {
            handle: Some(handle),
            sender: Some(tx)
        }
    }

    fn stop(&mut self) {
        if let Some(tx) = self.sender.take() {
            // Stopping the thread is not straightforward as Rust does not have a built-in way to forcefully kill threads.
            // Here we detach the thread, which means it will run indefinitely unless the process exits.
            tx.send(1);
        }
    }
}
