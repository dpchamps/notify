use futures::{
    channel::mpsc::{channel, Receiver, SendError},
};
use notify::{Config, Event, FsEventWatcher, RecommendedWatcher, RecursiveMode, Watcher};

use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
use std::{path::Path, time::Duration, sync::{RwLock, Arc}};

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
    event_state: RWEventState
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

    pub fn debounce(&mut self, seconds: u64) -> &mut Self {
        self.options.debounce_s = seconds;
        self
    }

    pub fn recursive(&mut self)-> &mut Self  {
        self.options.recursive_mode = FsWatchRecursiveMode::Recursive;
        self
    }

    pub fn path(&mut self, path: String) -> &mut Self   {
        self.options.path = path;
        self
    }

    pub fn build(self: Self) -> Result<FsWatch, String> {
        let (mut tx, receiver) = channel(1);
        let event_state: RWEventState  = Arc::new(RwLock::new(EventState::default()));
        let local_event_state = event_state;
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


struct FsWatching {
    watch: FsWatch,
    watcher: Box<dyn Watcher>
}



impl FsWatch {
    // pub fn watch(self) -> Result<FsWatching, String> {
    pub fn watch(self) -> Result<Box<dyn Watcher>, String> {
        let mut selfy = self;
        let watcher = selfy.debouncer.watcher();
        let recursive_mode = match self.options.recursive_mode {
            FsWatchRecursiveMode::Flat => RecursiveMode::NonRecursive,
            FsWatchRecursiveMode::Recursive => RecursiveMode::Recursive
        };
        watcher.watch(&Path::new(&self.options.path), recursive_mode).map_err(|e| e.to_string())?;
        let boxed = Box::new(watcher);
        Ok(FsWatching {
            watch: self,
            watcher: boxed,
        })
    }
}

async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let watch = FsWatchBuilder::new()
        .debounce(1)
        .path(".".to_owned())
        .build().unwrap();
    let x = watch.watch().unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    debouncer
        .watcher()
        .watch(path.as_ref(), RecursiveMode::Recursive)?;

    while let Some(res) = rx.next().await {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}
