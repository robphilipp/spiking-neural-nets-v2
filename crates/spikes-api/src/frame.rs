//! Live-data frames: one per viewer per display interval, little-endian, columnar, with every column starting on
//! an 8-byte boundary so a browser can wrap it in a typed array without copying.
//!
//! ```text
//! header (64 bytes)
//!   0  magic "SPKF"            4  version u16          6  flags u16
//!   8  session u32            12  frame_seq u32
//!  16  t0_ns u64              24  t1_ns u64            simulation interval the frame covers
//!  32  wall_unix_ms u64
//!  40  speed f32 (sim s per wall s; +inf = as fast as possible, 0 = not running)
//!  44  lag_ms f32 (simulation ms behind the clock)
//!  48  lod u8                 49  section_count u8     50  viewer u16 (which of the client's views)
//!  52  dropped_events u32     56  dropped_frames u32
//!  60  time_unit_ns u32       offsets are in this unit (1 unless the frame spans more than ~4.3 s)
//! sections, each:
//!   kind u16, reserved u16, count u32, bytes u32 (payload length), reserved u32, then the payload:
//!   one column after another, each padded to a multiple of 8 bytes
//! ```
//!
//! | kind | section | columns |
//! |------|---------|---------|
//! | 1 | spikes | neuron u32, offset u32 |
//! | 2 | spike counts | neuron u32, count u16 |
//! | 3 | spike bins | cell u32, count u32 |
//! | 4 | membrane | neuron u32, offset u32, mv f32 |
//! | 5 | weights | post u32, synapse u32, weight f32 |
//! | 6 | transitions | neuron u32, offset u32, tonic u8 |

use std::fmt;

/// The magic number at the start of every frame.
pub const MAGIC: [u8; 4] = *b"SPKF";
/// Header length in bytes.
pub const HEADER_BYTES: usize = 64;
/// Section header length in bytes.
pub const SECTION_HEADER_BYTES: usize = 16;

/// How much detail a frame's spikes have.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lod {
    /// Every spike, with its time.
    Exact = 0,
    /// Spikes per neuron in the frame.
    Counts = 1,
    /// Spikes per spatial cell in the frame.
    Binned = 2,
}

/// Section kinds.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SectionKind {
    /// Every spike: neuron, offset.
    Spikes = 1,
    /// Spikes per neuron: neuron, count.
    SpikeCounts = 2,
    /// Spikes per cell: cell, count.
    SpikeBins = 3,
    /// Membrane updates: neuron, offset, mV.
    Membrane = 4,
    /// Latest weights: post, synapse, weight.
    Weights = 5,
    /// Tonic transitions: neuron, offset, tonic.
    Transitions = 6,
}

impl SectionKind {
    fn from_u16(v: u16) -> Option<Self> {
        Some(match v {
            1 => Self::Spikes,
            2 => Self::SpikeCounts,
            3 => Self::SpikeBins,
            4 => Self::Membrane,
            5 => Self::Weights,
            6 => Self::Transitions,
            _ => return None,
        })
    }
}

/// A frame's header fields.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FrameHeader {
    /// Protocol version.
    pub version: u16,
    /// Session id.
    pub session: u32,
    /// Sequence number per viewer (gaps mean dropped frames).
    pub seq: u32,
    /// The viewer the frame is for (its id within the session, modulo 2¹⁶).
    pub viewer: u16,
    /// Start of the simulation interval covered, in ns.
    pub t0_ns: u64,
    /// End of the simulation interval covered, in ns.
    pub t1_ns: u64,
    /// Wall-clock time the frame was made, Unix ms.
    pub wall_unix_ms: u64,
    /// Simulated seconds per wall second; infinite as fast as possible; 0 when not running.
    pub speed: f32,
    /// How far simulation time is behind the clock, in simulation ms.
    pub lag_ms: f32,
    /// Spike level of detail.
    pub lod: Lod,
    /// Events lost before reaching the viewer so far (the engine's subscription fell behind).
    pub dropped_events: u32,
    /// Frames lost for this viewer so far (it read too slowly).
    pub dropped_frames: u32,
    /// The unit of offsets, in ns.
    pub time_unit_ns: u32,
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self {
            version: crate::PROTOCOL_VERSION,
            session: 0,
            seq: 0,
            viewer: 0,
            t0_ns: 0,
            t1_ns: 0,
            wall_unix_ms: 0,
            speed: 0.0,
            lag_ms: 0.0,
            lod: Lod::Exact,
            dropped_events: 0,
            dropped_frames: 0,
            time_unit_ns: 1,
        }
    }
}

/// Writes one column's bytes.
type ColumnWriter<'a> = dyn Fn(&mut Vec<u8>) + 'a;

/// Builds a frame.
#[derive(Debug)]
pub struct FrameBuilder {
    bytes: Vec<u8>,
    sections: u8,
}

fn pad8(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(8) {
        bytes.push(0);
    }
}

impl FrameBuilder {
    /// Starts a frame with this header (`section_count` is filled in by [`Self::finish`]).
    pub fn new(header: &FrameHeader) -> Self {
        let mut bytes = Vec::with_capacity(4096);
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&header.session.to_le_bytes());
        bytes.extend_from_slice(&header.seq.to_le_bytes());
        bytes.extend_from_slice(&header.t0_ns.to_le_bytes());
        bytes.extend_from_slice(&header.t1_ns.to_le_bytes());
        bytes.extend_from_slice(&header.wall_unix_ms.to_le_bytes());
        bytes.extend_from_slice(&header.speed.to_le_bytes());
        bytes.extend_from_slice(&header.lag_ms.to_le_bytes());
        bytes.push(header.lod as u8);
        bytes.push(0); // section count, set in finish
        bytes.extend_from_slice(&header.viewer.to_le_bytes());
        bytes.extend_from_slice(&header.dropped_events.to_le_bytes());
        bytes.extend_from_slice(&header.dropped_frames.to_le_bytes());
        bytes.extend_from_slice(&header.time_unit_ns.to_le_bytes());
        debug_assert_eq!(bytes.len(), HEADER_BYTES);
        Self { bytes, sections: 0 }
    }

    fn section(&mut self, kind: SectionKind, count: usize, columns: &[&ColumnWriter<'_>]) {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(&(kind as u16).to_le_bytes());
        self.bytes.extend_from_slice(&0u16.to_le_bytes());
        self.bytes.extend_from_slice(&(count as u32).to_le_bytes());
        self.bytes.extend_from_slice(&0u32.to_le_bytes()); // payload length, below
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
        let payload = self.bytes.len();
        for write in columns {
            write(&mut self.bytes);
            pad8(&mut self.bytes);
        }
        let len = (self.bytes.len() - payload) as u32;
        self.bytes[start + 8..start + 12].copy_from_slice(&len.to_le_bytes());
        self.sections += 1;
    }

    /// Adds exact spikes.
    pub fn spikes(&mut self, neuron: &[u32], offset: &[u32]) -> &mut Self {
        assert_eq!(neuron.len(), offset.len());
        self.section(
            SectionKind::Spikes,
            neuron.len(),
            &[&|b| put_u32(b, neuron), &|b| put_u32(b, offset)],
        );
        self
    }

    /// Adds spike counts per neuron.
    pub fn spike_counts(&mut self, neuron: &[u32], count: &[u16]) -> &mut Self {
        assert_eq!(neuron.len(), count.len());
        self.section(
            SectionKind::SpikeCounts,
            neuron.len(),
            &[&|b| put_u32(b, neuron), &|b| {
                count.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()))
            }],
        );
        self
    }

    /// Adds spike counts per cell.
    pub fn spike_bins(&mut self, cell: &[u32], count: &[u32]) -> &mut Self {
        assert_eq!(cell.len(), count.len());
        self.section(
            SectionKind::SpikeBins,
            cell.len(),
            &[&|b| put_u32(b, cell), &|b| put_u32(b, count)],
        );
        self
    }

    /// Adds membrane updates.
    pub fn membrane(&mut self, neuron: &[u32], offset: &[u32], mv: &[f32]) -> &mut Self {
        assert!(neuron.len() == offset.len() && neuron.len() == mv.len());
        self.section(
            SectionKind::Membrane,
            neuron.len(),
            &[&|b| put_u32(b, neuron), &|b| put_u32(b, offset), &|b| put_f32(b, mv)],
        );
        self
    }

    /// Adds weights.
    pub fn weights(&mut self, post: &[u32], synapse: &[u32], weight: &[f32]) -> &mut Self {
        assert!(post.len() == synapse.len() && post.len() == weight.len());
        self.section(
            SectionKind::Weights,
            post.len(),
            &[&|b| put_u32(b, post), &|b| put_u32(b, synapse), &|b| put_f32(b, weight)],
        );
        self
    }

    /// Adds tonic transitions.
    pub fn transitions(&mut self, neuron: &[u32], offset: &[u32], tonic: &[bool]) -> &mut Self {
        assert!(neuron.len() == offset.len() && neuron.len() == tonic.len());
        self.section(
            SectionKind::Transitions,
            neuron.len(),
            &[&|b| put_u32(b, neuron), &|b| put_u32(b, offset), &|b| {
                b.extend(tonic.iter().map(|&t| u8::from(t)))
            }],
        );
        self
    }

    /// The encoded frame.
    pub fn finish(mut self) -> Vec<u8> {
        self.bytes[49] = self.sections;
        self.bytes
    }
}

fn put_u32(b: &mut Vec<u8>, values: &[u32]) {
    values.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
}

fn put_f32(b: &mut Vec<u8>, values: &[f32]) {
    values.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
}

/// A decoded frame (for tests and Rust clients; browsers read the bytes in place).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The header.
    pub header: FrameHeader,
    /// Exact spikes: (neuron, offset).
    pub spikes: Vec<(u32, u32)>,
    /// Spikes per neuron.
    pub spike_counts: Vec<(u32, u16)>,
    /// Spikes per cell.
    pub spike_bins: Vec<(u32, u32)>,
    /// Membrane updates: (neuron, offset, mV).
    pub membrane: Vec<(u32, u32, f32)>,
    /// Weights: (post, synapse, weight).
    pub weights: Vec<(u32, u32, f32)>,
    /// Transitions: (neuron, offset, tonic).
    pub transitions: Vec<(u32, u32, bool)>,
}

impl Frame {
    /// The simulation time of an offset, in ns.
    pub fn time_ns(&self, offset: u32) -> u64 {
        self.header.t0_ns + u64::from(offset) * u64::from(self.header.time_unit_ns)
    }
}

/// A malformed frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameError(pub String);

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed frame: {}", self.0)
    }
}

impl std::error::Error for FrameError {}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FrameError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| FrameError(format!("truncated at byte {}", self.at)))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn column<const N: usize, T>(&mut self, count: usize, f: fn([u8; N]) -> T) -> Result<Vec<T>, FrameError> {
        let raw = self.take(count * N)?;
        let values = raw
            .chunks_exact(N)
            .map(|c| f(c.try_into().expect("chunk size")))
            .collect();
        self.take((8 - (count * N) % 8) % 8)?;
        Ok(values)
    }
}

fn arr<const N: usize>(b: &[u8]) -> [u8; N] {
    b.try_into().expect("slice length")
}

/// Decodes a frame.
pub fn decode(bytes: &[u8]) -> Result<Frame, FrameError> {
    if bytes.len() < HEADER_BYTES || bytes[0..4] != MAGIC {
        return Err(FrameError("not a frame".into()));
    }
    let h = &bytes[..HEADER_BYTES];
    let u16_at = |i: usize| u16::from_le_bytes(arr(&h[i..i + 2]));
    let u32_at = |i: usize| u32::from_le_bytes(arr(&h[i..i + 4]));
    let u64_at = |i: usize| u64::from_le_bytes(arr(&h[i..i + 8]));
    let f32_at = |i: usize| f32::from_le_bytes(arr(&h[i..i + 4]));
    let lod = match h[48] {
        0 => Lod::Exact,
        1 => Lod::Counts,
        2 => Lod::Binned,
        other => return Err(FrameError(format!("unknown level of detail {other}"))),
    };
    let mut frame = Frame {
        header: FrameHeader {
            version: u16_at(4),
            session: u32_at(8),
            seq: u32_at(12),
            viewer: u16_at(50),
            t0_ns: u64_at(16),
            t1_ns: u64_at(24),
            wall_unix_ms: u64_at(32),
            speed: f32_at(40),
            lag_ms: f32_at(44),
            lod,
            dropped_events: u32_at(52),
            dropped_frames: u32_at(56),
            time_unit_ns: u32_at(60),
        },
        ..Frame::default()
    };
    let mut r = Reader {
        bytes,
        at: HEADER_BYTES,
    };
    for _ in 0..h[49] {
        let s = r.take(SECTION_HEADER_BYTES)?;
        let kind = u16::from_le_bytes(arr(&s[0..2]));
        let count = u32::from_le_bytes(arr(&s[4..8])) as usize;
        let len = u32::from_le_bytes(arr(&s[8..12])) as usize;
        let end = r.at + len;
        let u32s = |r: &mut Reader<'_>| r.column::<4, u32>(count, u32::from_le_bytes);
        let f32s = |r: &mut Reader<'_>| r.column::<4, f32>(count, f32::from_le_bytes);
        match SectionKind::from_u16(kind) {
            Some(SectionKind::Spikes) => {
                let (n, o) = (u32s(&mut r)?, u32s(&mut r)?);
                frame.spikes = n.into_iter().zip(o).collect();
            }
            Some(SectionKind::SpikeCounts) => {
                let n = u32s(&mut r)?;
                let c = r.column::<2, u16>(count, u16::from_le_bytes)?;
                frame.spike_counts = n.into_iter().zip(c).collect();
            }
            Some(SectionKind::SpikeBins) => {
                let (c, n) = (u32s(&mut r)?, u32s(&mut r)?);
                frame.spike_bins = c.into_iter().zip(n).collect();
            }
            Some(SectionKind::Membrane) => {
                let (n, o, v) = (u32s(&mut r)?, u32s(&mut r)?, f32s(&mut r)?);
                frame.membrane = n.into_iter().zip(o).zip(v).map(|((n, o), v)| (n, o, v)).collect();
            }
            Some(SectionKind::Weights) => {
                let (p, s, w) = (u32s(&mut r)?, u32s(&mut r)?, f32s(&mut r)?);
                frame.weights = p.into_iter().zip(s).zip(w).map(|((p, s), w)| (p, s, w)).collect();
            }
            Some(SectionKind::Transitions) => {
                let (n, o) = (u32s(&mut r)?, u32s(&mut r)?);
                let t = r.column::<1, bool>(count, |[b]| b != 0)?;
                frame.transitions = n.into_iter().zip(o).zip(t).map(|((n, o), t)| (n, o, t)).collect();
            }
            None => {} // a newer section kind: skip it
        }
        if r.at != end {
            if r.at > end {
                return Err(FrameError(format!("section {kind} overruns its length")));
            }
            r.take(end - r.at)?;
        }
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let header = FrameHeader {
            session: 3,
            seq: 9,
            viewer: 2,
            t0_ns: 1_000,
            t1_ns: 17_000_000,
            wall_unix_ms: 123,
            speed: 0.5,
            lag_ms: 1.5,
            lod: Lod::Counts,
            dropped_events: 2,
            dropped_frames: 1,
            time_unit_ns: 1,
            ..FrameHeader::default()
        };
        let mut b = FrameBuilder::new(&header);
        b.spikes(&[1, 2, 3], &[10, 20, 30])
            .spike_counts(&[4, 5, 6], &[1, 2, 3])
            .spike_bins(&[7], &[70])
            .membrane(&[1], &[5], &[-1.5])
            .weights(&[1, 1], &[0, 1], &[0.5, 2.0])
            .transitions(&[9, 8, 7], &[1, 2, 3], &[true, false, true]);
        let bytes = b.finish();
        assert_eq!(bytes.len() % 8, 0);
        let frame = decode(&bytes).unwrap();
        assert_eq!(frame.header, header);
        assert_eq!(frame.spikes, [(1, 10), (2, 20), (3, 30)]);
        assert_eq!(frame.spike_counts, [(4, 1), (5, 2), (6, 3)]);
        assert_eq!(frame.spike_bins, [(7, 70)]);
        assert_eq!(frame.membrane, [(1, 5, -1.5)]);
        assert_eq!(frame.weights, [(1, 0, 0.5), (1, 1, 2.0)]);
        assert_eq!(frame.transitions, [(9, 1, true), (8, 2, false), (7, 3, true)]);
        assert_eq!(frame.time_ns(20), 1_020);
    }

    #[test]
    fn columns_start_on_8_byte_boundaries() {
        let mut b = FrameBuilder::new(&FrameHeader::default());
        b.spike_counts(&[1, 2, 3], &[1, 2, 3]).spikes(&[1], &[2]);
        let bytes = b.finish();
        // header 64 + section header 16 + neuron column 12→16 + count column 6→8, then the next section header.
        assert_eq!(&bytes[64 + 16 + 16 + 8..64 + 16 + 16 + 8 + 2], &1u16.to_le_bytes());
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decode(b"nope").is_err());
        let mut b = FrameBuilder::new(&FrameHeader::default());
        b.spikes(&[1, 2], &[3, 4]);
        let bytes = b.finish();
        assert!(decode(&bytes[..bytes.len() - 4]).is_err());
    }
}
