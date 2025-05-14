use reqwest::blocking::Client;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug)]
struct WebsiteStatus {
    url: String,
    action_status: Result<u16, String>,
    response_time: Duration,
    timestamp: SystemTime,
}

fn process_url(
    url: &str,
    client: &Client,
    timeout_secs: u64,
    retries: u32,
) -> WebsiteStatus {
    let start_time = Instant::now();
    let timestamp = SystemTime::now();
    let mut attempts = 0;
    let mut last_error = String::new();

    while attempts <= retries {
        let result = client.get(url).send();
        match result {
            Ok(resp) => {
                let status = resp.status();
                return WebsiteStatus {
                    url: url.to_string(),
                    action_status: Ok(status.as_u16()),
                    response_time: start_time.elapsed(),
                    timestamp,
                };
            }
            Err(e) => {
                last_error = format!("{}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
        attempts += 1;
    }

    WebsiteStatus {
        url: url.to_string(),
        action_status: Err(last_error),
        response_time: start_time.elapsed(),
        timestamp,
    }
}

fn write_status_json(results: &[WebsiteStatus]) {
    let mut json_output = String::from("[\n");

    for (i, result) in results.iter().enumerate() {
        let status_str = match &result.action_status {
            Ok(code) => format!("{}", code),
            Err(err) => format!("\"{}\"", err.replace('"', "'")),
        };

        let entry = format!(
            "  {{\n    \"url\": \"{}\",\n    \"status\": {},\n    \"response_time_ms\": {},\n    \"timestamp\": {}\n  }}",
            result.url,
            status_str,
            result.response_time.as_millis(),
            result.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
        );

        json_output.push_str(&entry);
        if i != results.len() - 1 {
            json_output.push_str(",\n");
        } else {
            json_output.push('\n');
        }
    }

    json_output.push(']');

    let mut file = File::create("status.json").expect("Failed to create status.json");
    file.write_all(json_output.as_bytes())
        .expect("Failed to write JSON");

    println!("Results written to status.json");
}

fn parse_args() -> Result<(Vec<String>, usize, u64, u32), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut urls = Vec::new();
    let mut workers = num_cpus::get();
    let mut timeout_secs = 5;
    let mut retries = 0;
    let mut file_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--file" => {
                i += 1;
                if i >= args.len() {
                    return Err("Expected file path after --file".to_string());
                }
                file_path = Some(args[i].clone());
            }
            "--workers" => {
                i += 1;
                workers = args[i].parse().map_err(|_| "Invalid --workers value".to_string())?;
            }
            "--timeout" => {
                i += 1;
                timeout_secs = args[i].parse().map_err(|_| "Invalid --timeout value".to_string())?;
            }
            "--retries" => {
                i += 1;
                retries = args[i].parse().map_err(|_| "Invalid --retries value".to_string())?;
            }
            arg if !arg.starts_with("--") => {
                urls.push(arg.to_string());
            }
            _ => return Err(format!("Unknown argument: {}", args[i])),
        }
        i += 1;
    }

    if let Some(path) = file_path {
        let content = fs::read_to_string(&path).map_err(|_| format!("Failed to read file {}", path))?;
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                urls.push(trimmed.to_string());
            }
        }
    }

    if urls.is_empty() {
        return Err("No URLs provided. Use --file or provide URLs as arguments.".to_string());
    }

    Ok((urls, workers, timeout_secs, retries))
}

fn main() {
    let (urls, num_workers, timeout_secs, retries) = match parse_args() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", e);
            eprintln!("Usage: website_checker [--file path] [URL ...] [--workers N] [--timeout S] [--retries N]");
            std::process::exit(2);
        }
    };

    println!(
        "Starting with {} workers, timeout {}s, retries {}",
        num_workers, timeout_secs, retries
    );

    let (tx, rx) = mpsc::channel::<String>();
    let rx = Arc::new(Mutex::new(rx));
    let results = Arc::new(Mutex::new(Vec::new()));

    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .expect("Failed to build client");
    let client = Arc::new(client);

    let mut workers_vec = Vec::new();
    for id in 0..num_workers {
        let rx_clone = Arc::clone(&rx);
        let client_clone = Arc::clone(&client);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            while let Ok(url) = rx_clone.lock().unwrap().recv() {
                let result = process_url(&url, &client_clone, timeout_secs, retries);
                match &result.action_status {
                    Ok(code) => println!(
                        "[Worker {}] {} -> HTTP {} ({} ms)",
                        id,
                        result.url,
                        code,
                        result.response_time.as_millis()
                    ),
                    Err(err) => println!(
                        "[Worker {}] {} -> Error: {} ({} ms)",
                        id,
                        result.url,
                        err,
                        result.response_time.as_millis()
                    ),
                }
                results_clone.lock().unwrap().push(result);
            }
            println!("[Worker {}] Exiting.", id);
        });

        workers_vec.push(handle);
    }

    for url in urls {
        tx.send(url).expect("Failed to send job");
    }
    drop(tx);

    for handle in workers_vec {
        handle.join().expect("Worker thread panicked");
    }

    println!("All checks complete.");
    let results = results.lock().unwrap();
    write_status_json(&results);
}
