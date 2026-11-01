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

### Run 4 (quiet machine)

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb | 201,963 req/s | 268 µs | 496 µs | 717 µs | 2,295 µs | 3,523 µs |
| plain axum | 200,677 req/s | 269 µs | 512 µs | 735 µs | 1,364 µs | 3,563 µs |

### Run 5 (quiet machine, after RTML integration)

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb | 210,750 req/s | 259 µs | 476 µs | 699 µs | 1,268 µs | 2,883 µs |
| plain axum | 208,173 req/s | 258 µs | 484 µs | 740 µs | 1,992 µs | 3,521 µs |

### FastAPI comparison (adjacent run, same machine)

Run immediately after Run 4, with the same route, load generator, connection
count, and CPU pinning.

- Python 3.15.1, FastAPI 0.162.0, uvicorn 0.54.0, two workers.
- uvicorn pinned to cores `0-1`, load generator to `2-5`.
- `GET /hello` returning a plain string, no Pydantic validation in the route.

| Server | Throughput | p50 | p90 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| RustWeb (Run 4) | 201,963 req/s | 268 µs | 496 µs | 717 µs | 2,295 µs | 3,523 µs |
| FastAPI + uvicorn (two runs) | 13,783–14,339 req/s | 4,083–4,263 µs | 4,343–4,535 µs | 4,519–4,971 µs | 8,319–8,479 µs | 8,791–10,399 µs |

In this setup RustWeb delivered roughly 14–15× the throughput and 6–7× lower
p99. This compares runtimes under the same load, not just the shape of the API;
it is still one machine and two runs, so treat the ratio as directional.

## Interpretation

Across five runs the RustWeb/axum throughput ratio was 1.014, 1.023, 0.969,
1.006, and 1.012 — between 3% slower and 2% faster, with no consistent
direction. That is run-to-run noise on a shared desktop, not framework
overhead. For routes without validation, the framework adds no measurable
request-path cost.

Absolute numbers vary with machine load: the same code measured 117k–211k
req/s depending on what else was running. The comparative result is the useful
one. These are loopback measurements on one machine, not a claim about absolute
performance or a TechEmpower ranking.

Validation cost is not included in this benchmark. `ApiJson<T>` adds
deserialization and `validator` work, which depends on the model and payload;
measure it separately for your workload before making latency claims.
