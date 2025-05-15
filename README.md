# Website Status Checker (Rust)

## Overview
This project is a concurrent website monitoring tool implemented in Rust.  
It checks the availability of websites in parallel using a fixed thread pool and reports the HTTP status, response time, and timestamp for each URL.

The program outputs results both live to the terminal and in a `status.json` file.

## Features
- Fixed worker thread pool using `std::thread` and `mpsc::channel`.
- Blocking HTTP requests using `reqwest` with per-request timeout.
- Configurable retries on failure with 100 ms delay.
- Safe concurrency using `Arc<Mutex>`.
- Manual JSON generation without external serialization crates.
- Clean CLI design and error handling (`Result<_, String>`).


## Usage

### Build
```bash
cargo run -- --file sites.txt --workers 10 --timeout 3 --retries 2
