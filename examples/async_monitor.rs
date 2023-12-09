use futures::{
    channel::mpsc::{channel, Receiver, SendError},
};
use notify::{Config, Event, FsEventWatcher, RecommendedWatcher, RecursiveMode, Watcher};

use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
use std::{path::Path, time::Duration, sync::{RwLock, Arc}};
use futures::{SinkExt, StreamExt};

/// Async, futures channel based event watching
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Argument 1 needs to be a path");
    println!("watching {}", path);

    futures::executor::block_on(async {
        if let Err(e) = async_watch(path).await {
            println!("error: {:?}", e)
        }
    });
}

struct FsWatchBuilder {
   options: FsWatchOptions
}

#[derive(Debug, Default)]
enum EventState {
    #[default]
    Sending,
    Halted(SendError)
}

type RWEventState = Arc<RwLock<EventState>>;

struct FsWatch {
    debouncer: Debouncer<FsEventWatcher>,
    receiver: Receiver<Result<Vec<DebouncedEvent>, notify::Error>>,
    event_state: RWEventState,
    options: FsWatchOptions
}


#[derive(Default)]
pub enum FsWatchRecursiveMode {
    #[default]
    Flat,
    Recursive,
}


struct FsWatchOptions {
    debounce_s: u64,
    recursive_mode: FsWatchRecursiveMode,
    path: String
}

impl Default for FsWatchOptions {
    fn default() -> Self {
        Self { debounce_s: Default::default(), recursive_mode: Default::default(), path: ".".to_string() }
    }
}

impl FsWatchBuilder {
    pub fn new() -> Self {
        Self { options: FsWatchOptions::default() }
    }

    pub fn debounce(mut self, seconds: u64) -> Self {
        self.options.debounce_s = seconds;
        self
    }

    pub fn recursive(mut self)-> Self  {
        self.options.recursive_mode = FsWatchRecursiveMode::Recursive;
        self
    }

    pub fn path(mut self, path: String) -> Self   {
        self.options.path = path;
        self
    }

    pub fn build(self: Self) -> Result<FsWatch, String> {
        let (mut tx, receiver) = channel(1);
        let event_state: RWEventState  = Arc::new(RwLock::new(EventState::default()));
        let local_event_state = event_state.clone();
        let debouncer: Debouncer<FsEventWatcher> =
            new_debouncer(Duration::from_secs(self.options.debounce_s), move |res| {
                futures::executor::block_on(async {
                    match tx.send(res).await {
                        Ok(_) => (),
                        Err(err) => {
                            if let Ok(mut current_state) = local_event_state.clone().write() {
                                *current_state = EventState::Halted(err);
                            }
                        }
                    }
                })
            }).map_err(|e| e.to_string())?;

        Ok(FsWatch {
            options: self.options,
            debouncer,
            receiver,
            event_state
        })
    }

}

struct FsWatching<'a> {
    watcher: &'a mut dyn Watcher
}

impl FsWatch {
    // pub fn watch(self) -> Result<FsWatching, String> {
    pub fn watch<'a>(&'a mut self) -> Result<FsWatching<'a>, String> {
        let watcher = self.debouncer.watcher();
        let recursive_mode = match self.options.recursive_mode {
            FsWatchRecursiveMode::Flat => RecursiveMode::NonRecursive,
            FsWatchRecursiveMode::Recursive => RecursiveMode::Recursive
        };
        watcher.watch(&Path::new(&self.options.path), recursive_mode).map_err(|e| e.to_string())?;
        Ok(FsWatching {
            watcher,
        })
    }
}

async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let mut watch = FsWatchBuilder::new()
        .debounce(1)
        .path(".".to_owned())
        .build().unwrap();

    let mut debouncer = watch.watch().unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    debouncer
        .watcher
        .watch(path.as_ref(), RecursiveMode::Recursive)?;

    while let Some(res) = watch.receiver.next().await {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}
