use std::time::{Duration, Instant};

use clap::Parser;
use hdrhistogram::Histogram;
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

#[derive(Debug, Clone, Parser)]
struct Config {
    /// Streaming endpoint, for example http://127.0.0.1:3001/stream
    #[arg(long)]
    url: String,

    /// Number of concurrent HTTP/1.1 connections.
    #[arg(long, default_value_t = 4)]
    connections: usize,

    /// Total number of streams to download.
    #[arg(long, default_value_t = 8)]
    streams: usize,

    /// Expected body size per stream; 0 disables the check.
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    expected_bytes: u64,
}

#[tokio::main]
async fn main() {
    let config = Config::parse();
    let uri: hyper::Uri = config.url.parse().expect("valid URL");
    let host = uri.host().expect("URL host").to_owned();
    let port = uri.port_u16().unwrap_or(80);
    let path = uri
        .path_and_query()
        .map(|value| value.as_str().to_owned())
        .unwrap_or_else(|| "/".to_owned());

    let total_streams = config.streams.max(1);
    let connections = config.connections.clamp(1, total_streams);
    let base = total_streams / connections;
    let extra = total_streams % connections;

    let started = Instant::now();
    let mut tasks = Vec::with_capacity(connections);

    for index in 0..connections {
        let count = base + usize::from(index < extra);
        let host = host.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            run_connection(host, port, path, count, config.expected_bytes).await
        }));
    }

    let mut bytes = 0u64;
    let mut mismatched = 0usize;
    let mut duration_secs = 0.0f64;
    let mut ttfb =
        Histogram::<u64>::new_with_bounds(1, 600_000_000, 3).expect("valid histogram bounds");
    let mut stream_time =
        Histogram::<u64>::new_with_bounds(1, 600_000_000, 3).expect("valid histogram bounds");

    for task in tasks {
        let worker = task.await.expect("worker task");
        bytes += worker.bytes;
        mismatched += worker.mismatched;
        duration_secs += worker.duration_secs;
        ttfb.add(&worker.ttfb).expect("compatible histograms");
        stream_time
            .add(&worker.stream_time)
            .expect("compatible histograms");
    }

    let elapsed = started.elapsed();
    let seconds = elapsed.as_secs_f64();
    let mib = bytes as f64 / (1024.0 * 1024.0);
    let mb = bytes as f64 / 1_000_000.0;
    let gbit = bytes as f64 * 8.0 / 1_000_000_000.0;

    println!("streams={total_streams} connections={connections} bytes={bytes} wall={seconds:.3}s");
    println!(
        "bandwidth={:.1} MiB/s ({:.1} MB/s, {:.2} Gbit/s)",
        mib / seconds,
        mb / seconds,
        gbit / seconds
    );
    println!(
        "per_stream_avg={:.1} MiB/s",
        if duration_secs > 0.0 {
            mib / duration_secs
        } else {
            0.0
        }
    );
    println!(
        "ttfb_us p50={} p90={} p99={}",
        ttfb.value_at_quantile(0.50),
        ttfb.value_at_quantile(0.90),
        ttfb.value_at_quantile(0.99)
    );
    println!(
        "stream_us p50={} p90={} p99={}",
        stream_time.value_at_quantile(0.50),
        stream_time.value_at_quantile(0.90),
        stream_time.value_at_quantile(0.99)
    );

    if mismatched > 0 {
        eprintln!(
            "error: {mismatched} stream(s) returned a different body length than {} bytes",
            config.expected_bytes
        );
        std::process::exit(1);
    }
}

struct Worker {
    bytes: u64,
    mismatched: usize,
    duration_secs: f64,
    ttfb: Histogram<u64>,
    stream_time: Histogram<u64>,
}

struct StreamStats {
    bytes: u64,
    ttfb_us: u64,
    elapsed_us: u64,
    elapsed_secs: f64,
}

async fn run_connection(
    host: String,
    port: u16,
    path: String,
    streams: usize,
    expected_bytes: u64,
) -> Worker {
    let stream = TcpStream::connect((host.as_str(), port))
        .await
        .expect("connect");
    stream.set_nodelay(true).expect("TCP_NODELAY");

    let io = TokioIo::new(stream);
    let (mut sender, connection) = http1::handshake(io).await.expect("HTTP handshake");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let mut worker = Worker {
        bytes: 0,
        mismatched: 0,
        duration_secs: 0.0,
        ttfb: Histogram::<u64>::new_with_bounds(1, 600_000_000, 3).expect("valid histogram bounds"),
        stream_time: Histogram::<u64>::new_with_bounds(1, 600_000_000, 3)
            .expect("valid histogram bounds"),
    };

    for _ in 0..streams {
        let stats = send_stream(&mut sender, &host, &path).await;
        worker.bytes += stats.bytes;
        worker.duration_secs += stats.elapsed_secs;
        worker
            .ttfb
            .record(stats.ttfb_us.clamp(1, 600_000_000))
            .expect("record ttfb");
        worker
            .stream_time
            .record(stats.elapsed_us.clamp(1, 600_000_000))
            .expect("record stream time");

        if expected_bytes > 0 && stats.bytes != expected_bytes {
            worker.mismatched += 1;
        }
    }

    worker
}

async fn send_stream(
    sender: &mut http1::SendRequest<Empty<Bytes>>,
    host: &str,
    path: &str,
) -> StreamStats {
    let request = Request::builder()
        .uri(path)
        .header("host", host)
        .body(Empty::new())
        .expect("valid request");

    let started = Instant::now();
    let response = sender.send_request(request).await.expect("send request");
    assert!(response.status().is_success(), "unexpected status");

    let mut body = response.into_body();
    let mut bytes = 0u64;
    let mut first_byte: Option<Duration> = None;

    while let Some(frame) = body.frame().await {
        let frame = frame.expect("body frame");
        if let Some(data) = frame.data_ref() {
            if first_byte.is_none() {
                first_byte = Some(started.elapsed());
            }
            bytes += data.len() as u64;
        }
    }

    let elapsed = started.elapsed();
    StreamStats {
        bytes,
        ttfb_us: first_byte
            .unwrap_or(elapsed)
            .as_micros()
            .min(u64::MAX as u128) as u64,
        elapsed_us: elapsed.as_micros().min(u64::MAX as u128) as u64,
        elapsed_secs: elapsed.as_secs_f64(),
    }
}
