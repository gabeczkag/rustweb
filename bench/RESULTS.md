# RustWeb benchmark — v0.1

**Date:** 2026-11-01
**Machine:** AMD Ryzen 5 4500U (6 cores), 7.1 GiB RAM, Linux 7.0.0-38-generic
**Toolchain:** rustc 1.99.1, release profile
**Load generator:** `bench-load` (hyper HTTP/1.1, keep-alive, 64 connections)

## Method

- Server pinned to cores `0-1`, load generator pinned to cores `2-5` with
  `taskset`.
- 10,000 warmup requests per run, 200,000 measured requests.
- Route: `GET /hello` returning `Hello, World!`.
- Two runs per server, executed in the same order each time.
- Both servers use the same axum/hyper/tokio stack; RustWeb adds only the
  generated wrapper that sets the status code and converts the response.

Run:

```bash
bash bench/run.sh
```

## Results

### Run 1

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb | 181,322 req/s | 295 µs | 551 µs | 781 µs | 1,446 µs | 3,619 µs |
| plain axum | 178,809 req/s | 298 µs | 553 µs | 786 µs | 1,414 µs | 3,447 µs |

### Run 2

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb | 182,958 req/s | 295 µs | 538 µs | 742 µs | 1,547 µs | 3,547 µs |
| plain axum | 178,759 req/s | 295 µs | 557 µs | 862 µs | 1,761 µs | 5,239 µs |

## Interpretation

The difference between RustWeb and plain axum is under 2% and changes sign
between runs — it is within run-to-run noise. For routes without validation,
the framework adds no measurable request-path overhead.

These numbers are loopback measurements on one machine, not a claim about
absolute performance or a TechEmpower ranking. They answer one question: does
the RustWeb layer itself cost anything? For this route, it does not.

Validation cost is not included in this benchmark. `ApiJson<T>` adds
deserialization and `validator` work, which depends on the model and payload;
measure it separately for your workload before making latency claims.
