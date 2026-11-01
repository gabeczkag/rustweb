# UC — compression packs

UC packs many small machine-readable packets into one binary blob. The blob is
opaque text, but any UC decoder can reconstruct the packets. It is compression
and framing, **not encryption**: use a real AEAD layer if the payload must stay
secret.

## Format

```text
magic     4   "UC01"
version   1   1
codec     1   0 = raw, 1 = zstd
flags     1   reserved, must be 0
packets   2   big-endian u16
raw       4   big-endian u32, uncompressed inner bytes
payload   4   big-endian u32, bytes after the 21-byte header
crc32     4   big-endian CRC32 of the inner bytes
payload   N   zstd frame or raw inner bytes
```

Inner packets are `kind: u8`, `length: u16 BE`, then `length` bytes.

Limits: **4096 bytes per packet** and **4 MiB per pack**. zstd level defaults to
9. A pack is stored as raw when compression would make it larger.

## CLI

Input JSON:

```json
{
  "packets": [
    { "kind": 1, "data": "bW92ZTp4PTEwLHk9MjA=" }
  ]
}
```

```bash
cargo run -p uc -- pack packets.json packets.uc --level 9
cargo run -p uc -- inspect packets.uc
cargo run -p uc -- unpack packets.uc packets.out.json
```

`data` is standard base64. The sample in `uc` tests packs 200 repeated
move-like packets from 25,925 raw bytes to 995 bytes (ratio 0.038); this is a
synthetic best case, not a promise for arbitrary data.

## HTTP service

```bash
cargo run -p uc -- serve --listen 127.0.0.1:4200 --level 9
```

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/healthz` | liveness |
| `GET` | `/status` | pack counts, bytes, ratio, uptime |
| `POST` | `/uc/pack` | JSON packets -> `application/x-uc-pack` |
| `POST` | `/uc/unpack` | binary pack -> JSON packets |

The pack response includes:

```text
X-UC-Codec: zstd
X-UC-Raw-Bytes: 25925
X-UC-Packed-Bytes: 995
X-UC-Packets: 200
X-UC-Ratio: 0.038380
```

The service has no authentication; keep it on localhost or behind a trusted
proxy. Compression is not a substitute for encryption or signatures.
