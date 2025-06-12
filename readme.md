# riptail

A Rust-based tailing tool that watches files and directories for changes and prints new lines in real time.

---

## ✅ Improvements in Latest Change

- Switched to `tokio::spawn` for concurrent file tailing with task tracking using `FuturesUnordered`.
- Added support for recursive and non-recursive directory watching via the `--recursive` flag.
- Implemented a clean structure around `RipTail` for tracking watched files and managing concurrent tasks.
- Used `Arc<Mutex<HashSet<PathBuf>>>` to safely prevent duplicate file watching across tasks.
- Automatically adds files from newly detected directories.
- Gracefully exits tailing tasks after 10 minutes of inactivity (`IDLE_TIME`).
- Displays colorized output using `colored` crate for better readability (path in blue, line number in purple).

---

## ⏳ Pending Work

- ☐ Improve error handling inside tailing logic (replace `unwrap` and `?` where possible with context-aware errors).
- ☐ Make the watch depth configurable via CLI (currently hardcoded to `1`).
- ☐ Support globs so they can be interpreted programatically
- ☐ Handle file rotation or deletion gracefully (e.g., log and remove from watchlist).
- ☐ Add a shutdown signal (e.g., Ctrl+C) to cancel all running tasks and clean up.
- ☐ Consider switching to `tokio::sync::Mutex` for better async compatibility.
- ☐ Add unit and integration tests for core logic in `RipTail`.

---
