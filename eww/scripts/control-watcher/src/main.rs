use std::{format, fs, println};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    loop {
        thread::sleep(Duration::from_secs(1));
        let ping = ping()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "0ms".to_string());

        println!("{}", serde_json::json!({"ping": ping, "uptime": uptime()}));
    }
}

fn ping() -> Option<u128> {
    let addr: SocketAddr = "1.1.1.1:443".parse().ok()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    Some(start.elapsed().as_millis().max(10).min(99))
}

fn uptime() -> String {
    let secs = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    let d = secs / 86400;
    let h = secs % 86400 / 3600;
    let m = secs % 3600 / 60;

    if d > 0 { format!("{d}d") }
    else if h > 0 { format!("{h}h") }
    else { format!("{m}m") }
}