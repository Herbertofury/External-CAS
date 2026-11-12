//! Serialized background ownership of private session reads/writes.
use external_cas_session::{LoadReport, SessionState, SessionStore, SnapshotReceipt};
use std::{path::PathBuf, sync::mpsc, thread::JoinHandle};

pub enum Reply {
    Loaded(Result<LoadReport, String>),
    Saved {
        state: SessionState,
        result: Result<SnapshotReceipt, String>,
    },
}
enum Command {
    Load,
    Save(SessionState),
    Stop,
}
pub struct Persistence {
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Reply>,
    worker: Option<JoinHandle<()>>,
}
impl Persistence {
    pub fn start(explicit_root: Option<PathBuf>) -> Result<Self, String> {
        let store = match explicit_root {
            Some(root) => SessionStore::at(root),
            None => SessionStore::open_default(),
        }
        .map_err(|e| e.to_string())?;
        let (tx, commands) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                match command {
                    Command::Load => {
                        let _ =
                            replies.send(Reply::Loaded(store.load().map_err(|e| e.to_string())));
                    }
                    Command::Save(state) => {
                        let result = store.save(&state).map_err(|e| e.to_string());
                        let _ = replies.send(Reply::Saved { state, result });
                    }
                    Command::Stop => break,
                }
            }
        });
        tx.send(Command::Load).map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            rx,
            worker: Some(worker),
        })
    }
    pub fn poll(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
    pub fn save(&self, state: SessionState) -> Result<(), String> {
        self.tx
            .send(Command::Save(state))
            .map_err(|e| e.to_string())
    }
}
impl Drop for Persistence {
    fn drop(&mut self) {
        // Stop follows all queued saves. An orderly app exit joins its owner;
        // no detached task can report durability after the process has exited.
        let _ = self.tx.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
