use std::fmt;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = *b"UC01";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 21;
pub const MAX_PACKET_BYTES: usize = 4096;
pub const MAX_PACK_BYTES: usize = 4 * 1024 * 1024;
pub const ZSTD_LEVEL: i32 = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub kind: u8,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketJson {
    pub kind: u8,
    /// Base64 standard, not URL-safe, so tools like base64(1) interoperate.
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Raw,
    Zstd,
}

impl Codec {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Zstd => "zstd",
        }
    }

    fn from_byte(value: u8) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Raw),
            1 => Ok(Self::Zstd),
            other => Err(Error::InvalidCodec(other)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackHeader {
    pub version: u8,
    pub codec: Codec,
    pub packet_count: u16,
    pub raw_bytes: u32,
    pub payload_bytes: u32,
    pub crc32: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackStats {
    pub packet_count: u16,
    pub raw_bytes: u32,
    pub packed_bytes: u32,
    pub payload_bytes: u32,
    pub codec: Codec,
    /// packed / raw; below 1.0 means the pack got smaller.
    pub ratio: f64,
}

#[derive(Debug, Clone)]
pub enum Error {
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidCodec(u8),
    TooManyPackets(usize),
    PacketTooLarge(usize),
    PackTooLarge(usize),
    LengthMismatch,
    CrcMismatch,
    Zstd(String),
    Base64(String),
    InvalidPacket(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => write!(formatter, "invalid UC magic"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported UC version {version}")
            }
            Self::InvalidCodec(codec) => write!(formatter, "invalid UC codec {codec}"),
            Self::TooManyPackets(count) => {
                write!(formatter, "too many packets in one UC pack: {count}")
            }
            Self::PacketTooLarge(size) => {
                write!(
                    formatter,
                    "packet is too large: {size} > {MAX_PACKET_BYTES}"
                )
            }
            Self::PackTooLarge(size) => {
                write!(formatter, "UC pack is too large: {size} > {MAX_PACK_BYTES}")
            }
            Self::LengthMismatch => write!(formatter, "UC length or count mismatch"),
            Self::CrcMismatch => write!(formatter, "UC CRC32 mismatch"),
            Self::Zstd(error) => write!(formatter, "UC zstd error: {error}"),
            Self::Base64(error) => write!(formatter, "UC base64 error: {error}"),
            Self::InvalidPacket(error) => write!(formatter, "UC packet error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn pack(packets: &[Packet]) -> Result<Vec<u8>, Error> {
    pack_with_level(packets, ZSTD_LEVEL)
}

pub fn pack_with_level(packets: &[Packet], level: i32) -> Result<Vec<u8>, Error> {
    if packets.len() > u16::MAX as usize {
        return Err(Error::TooManyPackets(packets.len()));
    }

    let mut raw = Vec::new();
    for packet in packets {
        if packet.data.len() > MAX_PACKET_BYTES {
            return Err(Error::PacketTooLarge(packet.data.len()));
        }
        if raw.len() + 3 + packet.data.len() > MAX_PACK_BYTES {
            return Err(Error::PackTooLarge(raw.len() + 3 + packet.data.len()));
        }
        raw.push(packet.kind);
        raw.extend_from_slice(&(packet.data.len() as u16).to_be_bytes());
        raw.extend_from_slice(&packet.data);
    }

    let compressed =
        zstd::bulk::compress(&raw, level).map_err(|error| Error::Zstd(error.to_string()))?;
    let (codec, payload) = if compressed.len() < raw.len() {
        (Codec::Zstd, compressed)
    } else {
        (Codec::Raw, raw.clone())
    };

    let mut hasher = Hasher::new();
    hasher.update(&raw);
    let crc32 = hasher.finalize();

    let mut output = Vec::with_capacity(HEADER_LEN + payload.len());
    output.extend_from_slice(&MAGIC);
    output.push(VERSION);
    output.push(match codec {
        Codec::Raw => 0,
        Codec::Zstd => 1,
    });
    output.push(0); // flags, reserved for future compression profiles
    output.extend_from_slice(&(packets.len() as u16).to_be_bytes());
    output.extend_from_slice(&(raw.len() as u32).to_be_bytes());
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    output.extend_from_slice(&crc32.to_be_bytes());
    output.extend_from_slice(&payload);

    Ok(output)
}

pub fn unpack(bytes: &[u8]) -> Result<Vec<Packet>, Error> {
    let header = inspect(bytes)?;
    let payload = &bytes[HEADER_LEN..];
    let raw = if header.codec == Codec::Zstd {
        zstd::bulk::decompress(payload, header.raw_bytes as usize)
            .map_err(|error| Error::Zstd(error.to_string()))?
    } else {
        payload.to_vec()
    };

    if raw.len() != header.raw_bytes as usize {
        return Err(Error::LengthMismatch);
    }

    let mut hasher = Hasher::new();
    hasher.update(&raw);
    if hasher.finalize() != header.crc32 {
        return Err(Error::CrcMismatch);
    }

    let mut packets = Vec::with_capacity(header.packet_count as usize);
    let mut offset = 0usize;
    for _ in 0..header.packet_count {
        if offset + 3 > raw.len() {
            return Err(Error::InvalidPacket("truncated packet header".to_owned()));
        }
        let kind = raw[offset];
        let length = u16::from_be_bytes([raw[offset + 1], raw[offset + 2]]) as usize;
        offset += 3;
        if length > MAX_PACKET_BYTES {
            return Err(Error::PacketTooLarge(length));
        }
        if offset + length > raw.len() {
            return Err(Error::InvalidPacket("truncated packet body".to_owned()));
        }
        packets.push(Packet {
            kind,
            data: raw[offset..offset + length].to_vec(),
        });
        offset += length;
    }

    if offset != raw.len() {
        return Err(Error::InvalidPacket("trailing bytes in pack".to_owned()));
    }

    Ok(packets)
}

pub fn inspect(bytes: &[u8]) -> Result<PackHeader, Error> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::LengthMismatch);
    }
    if bytes[..4] != MAGIC {
        return Err(Error::InvalidMagic);
    }
    if bytes[4] != VERSION {
        return Err(Error::UnsupportedVersion(bytes[4]));
    }

    let codec = Codec::from_byte(bytes[5])?;
    if bytes[6] != 0 {
        return Err(Error::InvalidPacket("unknown UC flags".to_owned()));
    }

    let packet_count = u16::from_be_bytes([bytes[7], bytes[8]]);
    let raw_bytes = u32::from_be_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]);
    let payload_bytes = u32::from_be_bytes([bytes[13], bytes[14], bytes[15], bytes[16]]);
    let crc32 = u32::from_be_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]);

    if raw_bytes as usize > MAX_PACK_BYTES {
        return Err(Error::PackTooLarge(raw_bytes as usize));
    }
    if bytes.len() != HEADER_LEN + payload_bytes as usize {
        return Err(Error::LengthMismatch);
    }

    Ok(PackHeader {
        version: VERSION,
        codec,
        packet_count,
        raw_bytes,
        payload_bytes,
        crc32,
    })
}

pub fn stats(bytes: &[u8]) -> Result<PackStats, Error> {
    let header = inspect(bytes)?;
    let ratio = if header.raw_bytes == 0 {
        1.0
    } else {
        bytes.len() as f64 / header.raw_bytes as f64
    };

    Ok(PackStats {
        packet_count: header.packet_count,
        raw_bytes: header.raw_bytes,
        packed_bytes: bytes.len() as u32,
        payload_bytes: header.payload_bytes,
        codec: header.codec,
        ratio,
    })
}

pub fn packets_to_json(packets: &[Packet]) -> Vec<PacketJson> {
    packets
        .iter()
        .map(|packet| PacketJson {
            kind: packet.kind,
            data: BASE64.encode(&packet.data),
        })
        .collect()
}

pub fn packets_from_json(packets: &[PacketJson]) -> Result<Vec<Packet>, Error> {
    packets
        .iter()
        .map(|packet| {
            Ok(Packet {
                kind: packet.kind,
                data: BASE64
                    .decode(&packet.data)
                    .map_err(|error| Error::Base64(error.to_string()))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packets() -> Vec<Packet> {
        (0..64)
            .map(|index| Packet {
                kind: (index % 5) as u8,
                data: format!("move:{index}:{}", "a".repeat(64)).into_bytes(),
            })
            .collect()
    }

    #[test]
    fn round_trips_packets() {
        let input = packets();
        let encoded = pack(&input).unwrap();
        let decoded = unpack(&encoded).unwrap();
        assert_eq!(input, decoded);
        assert_eq!(inspect(&encoded).unwrap().packet_count, 64);
    }

    #[test]
    fn compresses_repeated_packets() {
        let input = packets();
        let encoded = pack(&input).unwrap();
        let stats = stats(&encoded).unwrap();
        assert_eq!(stats.codec, Codec::Zstd);
        assert!(stats.packed_bytes < stats.raw_bytes);
        assert!(stats.ratio < 1.0);
    }

    #[test]
    fn rejects_corrupted_crc() {
        let input = packets();
        let mut encoded = pack(&input).unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        assert!(matches!(
            unpack(&encoded),
            Err(Error::CrcMismatch) | Err(Error::Zstd(_))
        ));
    }

    #[test]
    fn rejects_oversized_packet() {
        let packet = Packet {
            kind: 1,
            data: vec![0; MAX_PACKET_BYTES + 1],
        };
        assert!(matches!(pack(&[packet]), Err(Error::PacketTooLarge(_))));
    }
}
