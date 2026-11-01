use std::time::Instant;

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
    /// Target URL, for example http://127.0.0.1:3001/hello
    #[arg(long)]
    url: String,

    /// Number of concurrent keep-alive connections.
    #[arg(long, default_value_t = 64)]
    connections: usize,

    /// Total number of measured requests.
    #[arg(long, default_value_t = 200_000)]
    requests: usize,

    /// Total number of warmup requests (not measured).
    #[arg(long, default_value_t = 10_000)]
    warmup: usize,
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

    let per_connection = config.requests / config.connections.max(1);
    let warmup_per_connection = config.warmup / config.connections.max(1);

    let started = Instant::now();
    let mut tasks = Vec::with_capacity(config.connections.max(1));

    for _ in 0..config.connections.max(1) {
        let host = host.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            run_connection(host, port, path, per_connection, warmup_per_connection).await
        }));
    }

    let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
        .expect("valid histogram bounds");
    let mut measured = 0usize;

    for task in tasks {
        let worker = task.await.expect("worker task");
        measured += worker.len() as usize;
        histogram.add(&worker).expect("compatible histograms");
    }

    let elapsed = started.elapsed();
    let seconds = elapsed.as_secs_f64();
    let throughput = measured as f64 / seconds;

    println!(
        "requests={measured} connections={} elapsed={:.3}s throughput={:.0} req/s",
        config.connections,
        seconds,
        throughput
    );
    println!(
        "latency_us p50={} p90={} p99={} p99.9={} max={}",
        histogram.value_at_quantile(0.50),
        histogram.value_at_quantile(0.90),
        histogram.value_at_quantile(0.99),
        histogram.value_at_quantile(0.999),
        histogram.max()
    );
}

async fn run_connection(
    host: String,
    port: u16,
    path: String,
    requests: usize,
    warmup: usize,
) -> Histogram<u64> {
    let stream = TcpStream::connect((host.as_str(), port))
        .await
        .expect("connect");
    stream.set_nodelay(true).expect("TCP_NODELAY");

    let io = TokioIo::new(stream);
    let (mut sender, connection) = http1::handshake(io).await.expect("HTTP handshake");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let mut histogram =
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds");

    for _ in 0..warmup {
        send(&mut sender, &host, &path).await;
    }

    for _ in 0..requests {
        let started = Instant::now();
        send(&mut sender, &host, &path).await;
        let micros = started.elapsed().as_micros().min(60_000_000) as u64;
        histogram.record(micros).expect("record latency");
    }

    histogram
}

async fn send(sender: &mut http1::SendRequest<Empty<Bytes>>, host: &str, path: &str) {
    let request = Request::builder()
        .uri(path)
        .header("host", host)
        .body(Empty::new())
        .expect("valid request");

    let response = sender.send_request(request).await.expect("send request");
    assert!(response.status().is_success(), "unexpected status");
    let _ = response
        .into_body()
        .collect()
        .await
        .expect("drain response body");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config() {
        let config = Config::parse_from([
            "bench-load",
            "--url",
            "http://127.0.0.1:3000/hello",
            "--connections",
            "4",
            "--requests",
            "100",
        ]);

        assert_eq!(config.url, "http://127.0.0.1:3000/hello");
        assert_eq!(config.connections, 4);
        assert_eq!(config.requests, 100);
        assert_eq!(config.warmup, 10_000);
    }
}
