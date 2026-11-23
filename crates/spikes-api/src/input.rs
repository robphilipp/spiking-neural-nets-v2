//! Sensor input batches, client → engine, little-endian.
//!
//! ```text
//! header (32 bytes)
//!   0  magic "SPKS"          4  version u16          6  flags u16 (bit 0: events, not samples)
//!   8  sensor u32 (from ConnectSensor)              12  batch_seq u32
//!  16  t0_ns u64             the sensor's clock at the batch start
//!  24  channels u16         26  count u16 (samples per channel, or events)
//!  28  dt_ns u32             sample spacing (samples only)
//! payload
//!   samples: values f32[count × channels], sample-major (values[s × channels + c])
//!   events:  channel u16[count] (padded to 4 bytes), offset_ns u32[count] (from t0)
//! ```

use crate::frame::FrameError;

/// The magic number at the start of every input batch.
pub const MAGIC: [u8; 4] = *b"SPKS";
/// Header length in bytes.
pub const HEADER_BYTES: usize = 32;

/// A batch from a sensor.
#[derive(Clone, Debug, PartialEq)]
pub enum InputBatch {
    /// Samples on all channels at a fixed spacing.
    Samples {
        /// Sensor id.
        sensor: u32,
        /// Batch sequence number (for the sender's bookkeeping).
        seq: u32,
        /// The sensor's clock at the first sample, ns.
        t0_ns: u64,
        /// Sample spacing, ns.
        dt_ns: u32,
        /// Channels.
        channels: u16,
        /// Sample-major values.
        values: Vec<f32>,
    },
    /// Events from a sensor that produces spikes.
    Events {
        /// Sensor id.
        sensor: u32,
        /// Batch sequence number.
        seq: u32,
        /// The sensor's clock at the batch start, ns.
        t0_ns: u64,
        /// (channel, offset from `t0_ns` in ns).
        events: Vec<(u16, u32)>,
    },
}

impl InputBatch {
    /// The sensor it's from.
    pub fn sensor(&self) -> u32 {
        match self {
            Self::Samples { sensor, .. } | Self::Events { sensor, .. } => *sensor,
        }
    }

    /// Encodes the batch.
    ///
    /// # Panics
    /// If a samples batch's values aren't whole samples of `channels`, or a batch has more than 65 535 samples or
    /// events.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC);
        b.extend_from_slice(&crate::PROTOCOL_VERSION.to_le_bytes());
        match self {
            Self::Samples {
                sensor,
                seq,
                t0_ns,
                dt_ns,
                channels,
                values,
            } => {
                assert!(*channels > 0 && values.len().is_multiple_of(usize::from(*channels)));
                let count = u16::try_from(values.len() / usize::from(*channels)).expect("at most 65 535 samples");
                header(&mut b, 0, *sensor, *seq, *t0_ns, *channels, count, *dt_ns);
                values.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
            }
            Self::Events {
                sensor,
                seq,
                t0_ns,
                events,
            } => {
                let count = u16::try_from(events.len()).expect("at most 65 535 events");
                header(&mut b, 1, *sensor, *seq, *t0_ns, 0, count, 0);
                events.iter().for_each(|e| b.extend_from_slice(&e.0.to_le_bytes()));
                while !b.len().is_multiple_of(4) {
                    b.push(0);
                }
                events.iter().for_each(|e| b.extend_from_slice(&e.1.to_le_bytes()));
            }
        }
        b
    }

    /// Decodes a batch.
    pub fn decode(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() < HEADER_BYTES || bytes[0..4] != MAGIC {
            return Err(FrameError("not a sensor input batch".into()));
        }
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().expect("4 bytes"));
        let flags = u16_at(6);
        let sensor = u32_at(8);
        let seq = u32_at(12);
        let t0_ns = u64::from_le_bytes(bytes[16..24].try_into().expect("8 bytes"));
        let channels = u16_at(24);
        let count = usize::from(u16_at(26));
        let dt_ns = u32_at(28);
        let payload = &bytes[HEADER_BYTES..];
        if flags & 1 == 0 {
            let n = count * usize::from(channels);
            if channels == 0 || payload.len() != n * 4 {
                return Err(FrameError(format!(
                    "{count} samples × {channels} channels need {} payload bytes, got {}",
                    n * 4,
                    payload.len()
                )));
            }
            let values = payload
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().expect("4 bytes")))
                .collect();
            Ok(Self::Samples {
                sensor,
                seq,
                t0_ns,
                dt_ns,
                channels,
                values,
            })
        } else {
            let channel_bytes = (count * 2).div_ceil(4) * 4;
            if payload.len() != channel_bytes + count * 4 {
                return Err(FrameError(format!(
                    "{count} events don't match {} payload bytes",
                    payload.len()
                )));
            }
            let channels = payload[..count * 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]));
            let offsets = payload[channel_bytes..]
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes(c.try_into().expect("4 bytes")));
            Ok(Self::Events {
                sensor,
                seq,
                t0_ns,
                events: channels.zip(offsets).collect(),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn header(b: &mut Vec<u8>, flags: u16, sensor: u32, seq: u32, t0_ns: u64, channels: u16, count: u16, dt_ns: u32) {
    b.extend_from_slice(&flags.to_le_bytes());
    b.extend_from_slice(&sensor.to_le_bytes());
    b.extend_from_slice(&seq.to_le_bytes());
    b.extend_from_slice(&t0_ns.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&count.to_le_bytes());
    b.extend_from_slice(&dt_ns.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let samples = InputBatch::Samples {
            sensor: 3,
            seq: 7,
            t0_ns: 1 << 40,
            dt_ns: 1_000_000,
            channels: 2,
            values: vec![0.5, -1.0, 2.0, 3.5],
        };
        assert_eq!(InputBatch::decode(&samples.encode()).unwrap(), samples);
        let events = InputBatch::Events {
            sensor: 1,
            seq: 0,
            t0_ns: 5,
            events: vec![(0, 10), (3, 20), (1, 30)],
        };
        let bytes = events.encode();
        assert_eq!(bytes.len(), 32 + 8 + 12);
        assert_eq!(InputBatch::decode(&bytes).unwrap(), events);
    }

    #[test]
    fn malformed_batches_are_rejected() {
        let mut bytes = InputBatch::Samples {
            sensor: 0,
            seq: 0,
            t0_ns: 0,
            dt_ns: 1,
            channels: 2,
            values: vec![1.0, 2.0],
        }
        .encode();
        bytes.pop();
        assert!(InputBatch::decode(&bytes).is_err());
        assert!(InputBatch::decode(b"SPKF").is_err());
    }
}
