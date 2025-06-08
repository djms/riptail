#![allow(unused_imports)]
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use futures::stream::FuturesUnordered;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};
use tokio::time::{Duration, Instant, sleep};

// cargo add tokio --features tokio/full tokio_stream regex clap --features clap/derive notify futures colored anyhow
#[derive(Parser, Debug)]
struct Args {
    /// Directory (or glob pattern) to watch
    #[arg(default_value = ".")]
    path: String,
}

const IDLE_TIME: u64 = 60 * 10; // 10 min Without logs

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let root = PathBuf::from(&args.path);
    let watched_files = Arc::new(Mutex::new(HashSet::new()));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send((args.path.clone(), path));
                }
            }
        })?;
    watcher.watch(&root, RecursiveMode::NonRecursive)?;
    let tasks = FuturesUnordered::new(); // is internally mutable (using Pin + poll_next)

    while let Some(stem_path) = rx.recv().await {
        let (stem, path) = stem_path;
        if path.is_file() {
            let was_new = {
                let mut set = watched_files.lock().unwrap();
                set.insert(path.clone())
            };
            if was_new {
                tasks.push(tokio::spawn(async move {
                    if let Err(e) = tail_file(stem, path).await {
                        eprintln!("Error tailing file: {e}");
                    }
                }));
            }
        }
    }
    Ok(())
}

async fn tail_file(stem: String, path: PathBuf) -> Result<()> {
    // let path = Path::new("/Users/danieljm/google_drive/study/rust/projects/riptail/h.txt");
    let mut byte_offset = 0u64;
    let mut buffer = Vec::new();
    let mut to = Instant::now();
    let base = fs::canonicalize(stem)?;
    let child = fs::canonicalize(&path)?;
    let relative = &child.strip_prefix(&base)?;
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
            sleep(Duration::from_secs(1)).await;
        } else {
            row_n += 1;
            to = Instant::now();
            byte_offset += bytes_read as u64;
            let cow_line = String::from_utf8_lossy(&buffer); // Copy On Write
            println!(
                "{}:{}| {}",
                relative.display().to_string().blue().bold(),
                row_n.to_string().purple().bold(),
                cow_line.trim_end()
            );
        }
    }
    Ok(())
}
