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

### Run 3 (machine under background load)

Run 3 was executed while the desktop had a browser, Xorg, and the editor running
(load average 4.7 on 6 cores). Absolute throughput dropped for both servers, but
the relative result stayed in the same band.

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb | 116,651 req/s | 441 µs | 820 µs | 1,979 µs | 3,919 µs | 13,519 µs |
| plain axum | 120,417 req/s | 449 µs | 773 µs | 1,938 µs | 3,765 µs | 10,079 µs |

## Interpretation

Across three runs the RustWeb/axum throughput ratio was 1.014, 1.023, and
0.969 — between 3% slower and 2% faster, with no consistent direction. That is
run-to-run noise on a shared desktop, not framework overhead. For routes without
validation, the framework adds no measurable request-path cost.

Absolute numbers vary with machine load: the same code measured 117k–183k
req/s depending on what else was running. The comparative result is the useful
one. These are loopback measurements on one machine, not a claim about absolute
performance or a TechEmpower ranking.

Validation cost is not included in this benchmark. `ApiJson<T>` adds
deserialization and `validator` work, which depends on the model and payload;
measure it separately for your workload before making latency claims.
