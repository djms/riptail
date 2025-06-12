#![allow(unused_imports)]
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use futures::stream::FuturesUnordered;
use notify::{
    RecommendedWatcher,
    RecursiveMode::{NonRecursive, Recursive},
    Watcher,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, sleep};
use walkdir::WalkDir;

const IDLE_TIME: u64 = 60 * 10; // 10 min without activity

// cargo add tokio --features tokio/full tokio_stream walkdir regex clap --features clap/derive notify futures colored anyhow
#[derive(Parser, Debug)]
struct Args {
    /// Directory (or glob pattern) to watch
    #[arg(value_name = "PATH", required = true)]
    path: Vec<PathBuf>,
    /// Watch directories recursively
    #[arg(short, long)]
    recursive: bool,
    /// Depth of directory recursion (default 1)
    #[arg(short, long, default_value_t = 1)]
    depth: usize,
}

struct RipTail {
    watched_files: Arc<Mutex<HashSet<PathBuf>>>,
    tasks: Arc<FuturesUnordered<JoinHandle<()>>>, // no need for mutex, is internally mutable (using Pin + poll_next)
}

impl RipTail {
    fn new() -> Self {
        RipTail {
            watched_files: Arc::new(Mutex::new(HashSet::new())),
            tasks: Arc::new(FuturesUnordered::new()),
        }
    }

    /// Returns true if file is already in the hashset
    async fn _set_file(&self, path: PathBuf) -> bool {
        let mut set = self.watched_files.lock().await;
        set.insert(path)
    }

    fn _clone_for_task(&self) -> RipTail {
        RipTail {
            watched_files: Arc::clone(&self.watched_files),
            tasks: Arc::clone(&self.tasks),
        }
    }

    async fn _watch_file(&self, path: PathBuf) -> Result<()> {
        let path = std::fs::canonicalize(path)?;
        let this = self._clone_for_task();
        let was_new = this._set_file(path.clone()).await;
        if was_new {
            let handle = tokio::spawn(async move {
                if let Err(e) = tail_file(path).await {
                    eprintln!("Error tailing file: {e}");
                }
            });
            this.tasks.push(handle);
        }
        Ok(())
    }

    async fn _watch_folder(&self, path: PathBuf, depth: usize) -> Result<()> {
        for entry in WalkDir::new(&path)
            .max_depth(depth)
            .into_iter()
            .filter_map(Result::ok)
        {
            let entry_path = entry.path();
            if entry_path.is_file() {
                self._watch_file(entry_path.to_path_buf()).await?;
            }
        }
        Ok(())
    }

    async fn watch(&self, path: PathBuf, depth: usize) -> Result<()> {
        if path.is_file() {
            self._watch_file(path).await?;
        } else if path.is_dir() {
            self._watch_folder(path, depth).await?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let paths = args.path.clone();
    // let watched_files = Arc::new(Mutex::new(HashSet::new()));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send(path);
                }
            }
        })?;

    let rt = RipTail::new();
    for path in paths {
        let mode = [NonRecursive, Recursive][args.recursive as usize];
        watcher.watch(&path, mode)?;
        rt.watch(path, args.depth).await?;
    }

    while let Some(path) = rx.recv().await {
        rt.watch(path, args.depth).await?;
    }
    Ok(())
}

async fn tail_file(path: PathBuf) -> Result<()> {
    let mut byte_offset = 0u64;
    let mut buffer = Vec::new();
    let mut to = Instant::now();
    let mut row_n = 0;
    loop {
        let file = File::open(&path).await?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(byte_offset)).await?;
        buffer.clear();
        let bytes_read = reader.read_until(b'\n', &mut buffer).await?;
        if bytes_read == 0 {
            if to.elapsed() > Duration::from_secs(IDLE_TIME) {
                break;
            }
            sleep(Duration::from_millis(300)).await;
        } else {
            row_n += 1;
            to = Instant::now();
            byte_offset += bytes_read as u64;
            let cow_line = String::from_utf8_lossy(&buffer); // Copy On Write
            println!(
                "{}:{}: {}",
                path.display().to_string().blue().bold(),
                row_n.to_string().purple().bold(),
                cow_line.trim_end()
            );
        }
    }
    Ok(())
}
