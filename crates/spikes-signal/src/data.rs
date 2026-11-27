//! Data files for formulas' `data("name", t, c)` (app plan 05 §3): a CSV table or a WAV recording, read into
//! channels of samples that a formula looks up in time with linear interpolation. The UI has the same parsers
//! (`apps/spikes-ui/src/dataFile.ts`); `tests/parity.rs` checks the two give the same lookups.
//!
//! - **CSV**: comma, semicolon or tab separated (the first line decides); an optional header row (a first row with a
//!   cell that isn't a number); blank lines and lines starting with `#` are skipped. The first column is time, in
//!   ms unless the header's first cell is `t_s` or `time_s` (seconds); every other column is a channel. Numbers
//!   only; times strictly ascending; every row as wide as the first.
//! - **WAV**: RIFF/WAVE, PCM 8 (unsigned), 16, 24 or 32 bit integers, or 32 or 64 bit floats; any number of
//!   channels; time is the sample's index over the sample rate from the header. Integers are scaled to [−1, 1).
//!
//! A lookup is 0 before the first sample and after the last, and for a channel the file doesn't have.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A data file as given to the engine: its name in formulas, and its bytes (CSV or WAV, told apart by content).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Why a data file couldn't be read: names the file and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataError(pub String);

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DataError {}

/// Where a data set's samples are in time.
#[derive(Clone, Debug, PartialEq)]
enum Timing {
    /// A time (ms) per row.
    Rows(Vec<f64>),
    /// Evenly: this many samples per second.
    Rate(f64),
}

/// A data file's samples: channels of equal length.
#[derive(Clone, Debug, PartialEq)]
pub struct DataSet {
    timing: Timing,
    channels: Vec<Vec<f32>>,
}

impl DataSet {
    /// Reads `bytes` as WAV if they start with `RIFF`, else as CSV. `name` is for the errors.
    ///
    /// # Errors
    /// A [`DataError`] saying what's wrong with the file.
    pub fn parse(name: &str, bytes: &[u8]) -> Result<Self, DataError> {
        let fail = |m: String| DataError(format!("data file '{name}': {m}"));
        if bytes.starts_with(b"RIFF") {
            parse_wav(bytes).map_err(fail)
        } else {
            parse_csv(bytes).map_err(fail)
        }
    }

    /// The number of channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// The number of samples (rows) per channel.
    pub fn len(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Samples per second for a WAV, `None` for a CSV.
    pub fn rate_hz(&self) -> Option<f64> {
        match self.timing {
            Timing::Rate(r) => Some(r),
            Timing::Rows(_) => None,
        }
    }

    /// The time of the last sample, ms (the first is at 0 for a WAV).
    pub fn duration_ms(&self) -> f64 {
        match &self.timing {
            Timing::Rows(t) => t.last().copied().unwrap_or(0.0),
            Timing::Rate(r) => self.len().saturating_sub(1) as f64 * 1000.0 / r,
        }
    }

    /// The time of the first sample, ms.
    pub fn start_ms(&self) -> f64 {
        match &self.timing {
            Timing::Rows(t) => t.first().copied().unwrap_or(0.0),
            Timing::Rate(_) => 0.0,
        }
    }

    /// The value of channel `c` at `t_ms`, linearly interpolated between samples; 0 outside the samples' time, for
    /// a channel the file doesn't have, and for a time that isn't finite.
    pub fn at(&self, t_ms: f64, c: usize) -> f64 {
        let Some(values) = self.channels.get(c) else { return 0.0 };
        if values.is_empty() || !t_ms.is_finite() {
            return 0.0;
        }
        let last = values.len() - 1;
        match &self.timing {
            Timing::Rows(times) => {
                let after = times.partition_point(|&x| x <= t_ms);
                if after == 0 {
                    return 0.0;
                }
                if after > last {
                    return if t_ms == times[last] {
                        f64::from(values[last])
                    } else {
                        0.0
                    };
                }
                let (t0, t1) = (times[after - 1], times[after]);
                let (v0, v1) = (f64::from(values[after - 1]), f64::from(values[after]));
                v0 + (v1 - v0) * ((t_ms - t0) / (t1 - t0))
            }
            Timing::Rate(rate) => {
                let pos = t_ms * rate / 1000.0;
                if pos < 0.0 || pos > last as f64 {
                    return 0.0;
                }
                let i = pos.floor() as usize;
                if i >= last {
                    return f64::from(values[last]);
                }
                let (v0, v1) = (f64::from(values[i]), f64::from(values[i + 1]));
                v0 + (v1 - v0) * (pos - i as f64)
            }
        }
    }
}

/// The data sets a formula can look up, by name.
#[derive(Clone, Debug, Default)]
pub struct DataSets(HashMap<String, Arc<DataSet>>);

impl DataSets {
    /// Reads every file.
    ///
    /// # Errors
    /// The first file that can't be read, or two files with one name.
    pub fn load(files: &[DataFile]) -> Result<Self, DataError> {
        let mut sets = HashMap::new();
        for f in files {
            if sets
                .insert(f.name.clone(), Arc::new(DataSet::parse(&f.name, &f.bytes)?))
                .is_some()
            {
                return Err(DataError(format!("two data files are named '{}'", f.name)));
            }
        }
        Ok(Self(sets))
    }

    pub fn get(&self, name: &str) -> Option<&DataSet> {
        self.0.get(name).map(AsRef::as_ref)
    }
}

// ----- CSV -----

fn parse_csv(bytes: &[u8]) -> Result<DataSet, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not text (is it a CSV or a WAV file?)".to_string())?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .peekable();
    let Some(&(_, first)) = lines.peek() else {
        return Err("it has no rows".into());
    };
    let delimiter = if first.contains('\t') {
        '\t'
    } else if first.contains(';') {
        ';'
    } else {
        ','
    };
    let mut seconds = false;
    let header_cells: Vec<&str> = first.split(delimiter).map(str::trim).collect();
    if header_cells.iter().any(|c| number(c).is_none()) {
        seconds = matches!(header_cells[0], "t_s" | "time_s");
        lines.next();
    }
    let mut times: Vec<f64> = Vec::new();
    let mut channels: Vec<Vec<f32>> = Vec::new();
    for (line_no, line) in lines {
        let cells: Vec<&str> = line.split(delimiter).map(str::trim).collect();
        if cells.len() < 2 {
            return Err(format!(
                "line {line_no}: needs a time column and at least one channel column"
            ));
        }
        if channels.is_empty() {
            channels = vec![Vec::new(); cells.len() - 1];
        } else if cells.len() != channels.len() + 1 {
            return Err(format!(
                "line {line_no}: has {} columns, the first row has {}",
                cells.len(),
                channels.len() + 1
            ));
        }
        let mut values = Vec::with_capacity(cells.len());
        for (col, cell) in cells.iter().enumerate() {
            values.push(
                number(cell).ok_or_else(|| format!("line {line_no}, column {}: '{cell}' is not a number", col + 1))?,
            );
        }
        let t = if seconds { values[0] * 1000.0 } else { values[0] };
        if times.last().is_some_and(|&last| t <= last) {
            return Err(format!("line {line_no}: times must be ascending"));
        }
        times.push(t);
        for (c, v) in values[1..].iter().enumerate() {
            channels[c].push(*v as f32);
        }
    }
    if times.is_empty() {
        return Err("it has no rows".into());
    }
    Ok(DataSet {
        timing: Timing::Rows(times),
        channels,
    })
}

/// A finite number (`1`, `-2.5`, `.5`, `1e3`), as JavaScript's `Number` reads it.
fn number(s: &str) -> Option<f64> {
    let digits = s.bytes().filter(u8::is_ascii_digit).count();
    let plain = s
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E'));
    if digits == 0 || !plain {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

// ----- WAV -----

fn parse_wav(b: &[u8]) -> Result<DataSet, String> {
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    if b.len() < 12 || &b[8..12] != b"WAVE" {
        return Err("not a WAV file (no RIFF/WAVE header)".into());
    }
    let mut format: Option<(u16, usize, u32, usize)> = None; // tag, channels, rate, bits
    let mut samples: Option<&[u8]> = None;
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32_at(i + 4) as usize;
        let body = i + 8;
        let end = body.saturating_add(size).min(b.len());
        if id == b"fmt " {
            if end - body < 16 {
                return Err("the fmt chunk is too short".into());
            }
            let mut tag = u16_at(body);
            if tag == 0xFFFE && end - body >= 26 {
                tag = u16_at(body + 24); // extensible: the sub-format's first two bytes
            }
            format = Some((
                tag,
                usize::from(u16_at(body + 2)),
                u32_at(body + 4),
                usize::from(u16_at(body + 14)),
            ));
        } else if id == b"data" {
            samples = Some(&b[body..end]);
        }
        i = body.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = format.ok_or("there is no fmt chunk")?;
    let data = samples.ok_or("there is no data chunk")?;
    if channels == 0 || rate == 0 {
        return Err("the header has no channels or no sample rate".into());
    }
    let float = match (tag, bits) {
        (1, 8 | 16 | 24 | 32) => false,
        (3, 32 | 64) => true,
        _ => {
            return Err(format!(
                "unsupported format (code {tag}, {bits} bits): PCM 8, 16, 24 or 32 bit integers, or 32 or 64 bit floats"
            ));
        }
    };
    let width = bits / 8;
    let frames = data.len() / (width * channels);
    if frames == 0 {
        return Err("it has no samples".into());
    }
    let mut out = vec![Vec::with_capacity(frames); channels];
    for f in 0..frames {
        for (c, column) in out.iter_mut().enumerate() {
            let p = (f * channels + c) * width;
            let s = &data[p..p + width];
            let v: f64 = match (float, bits) {
                (false, 8) => (f64::from(s[0]) - 128.0) / 128.0,
                (false, 16) => f64::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0,
                (false, 24) => {
                    let v = (i32::from(s[2] as i8) << 16) | (i32::from(s[1]) << 8) | i32::from(s[0]);
                    f64::from(v) / 8_388_608.0
                }
                (false, _) => f64::from(i32::from_le_bytes([s[0], s[1], s[2], s[3]])) / 2_147_483_648.0,
                (true, 32) => f64::from(f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
                (true, _) => f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]),
            };
            column.push(if v.is_finite() { v as f32 } else { 0.0 });
        }
    }
    Ok(DataSet {
        timing: Timing::Rate(f64::from(rate)),
        channels: out,
    })
}

/// A 16-bit PCM WAV file of `channels` (one `Vec` per channel, equal lengths): for tests and examples.
pub fn wav_16(rate: u32, channels: &[Vec<i16>]) -> Vec<u8> {
    let frames = channels.first().map_or(0, Vec::len);
    let n = channels.len() as u16;
    let data_len = (frames * channels.len() * 2) as u32;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(n) * 2).to_le_bytes());
    out.extend((n * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for f in 0..frames {
        for c in channels {
            out.extend(c[f].to_le_bytes());
        }
    }
    out
}

/// Decodes standard base64 (padding optional, whitespace ignored): how data files travel in JSON.
///
/// # Errors
/// For a character that isn't base64.
pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in text.bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return Err(format!("'{}' is not base64", ch as char)),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(text: &str) -> Result<DataSet, DataError> {
        DataSet::parse("d", text.as_bytes())
    }

    #[test]
    fn csv_with_a_header_and_interpolation() {
        let d = csv("t,a,b\n0,0,10\n100,1,20\n300,0,20\n").unwrap();
        assert_eq!((d.channel_count(), d.len(), d.duration_ms()), (2, 3, 300.0));
        assert_eq!(d.at(50.0, 0), 0.5);
        assert_eq!(d.at(100.0, 0), 1.0);
        assert_eq!(d.at(200.0, 0), 0.5);
        assert_eq!(d.at(50.0, 1), 15.0);
        assert_eq!(d.at(300.0, 1), 20.0, "the last sample itself");
        assert_eq!(d.at(-1.0, 0), 0.0, "before the first");
        assert_eq!(d.at(300.5, 0), 0.0, "after the last");
        assert_eq!(d.at(50.0, 2), 0.0, "a channel it doesn't have");
        assert_eq!(d.at(f64::NAN, 0), 0.0);
    }

    #[test]
    fn csv_delimiters_seconds_and_comments() {
        let d = csv("# note\ntime_s;x\n0;1\n0.5;3\n").unwrap();
        assert_eq!(d.duration_ms(), 500.0);
        assert_eq!(d.at(250.0, 0), 2.0);
        let d = csv("0\t1\t2\n10\t3\t4").unwrap();
        assert_eq!((d.channel_count(), d.at(5.0, 1)), (2, 3.0));
        let d = csv("\u{feff}1,5\n2,7\n").unwrap();
        assert_eq!((d.start_ms(), d.at(0.5, 0)), (1.0, 0.0));
    }

    #[test]
    fn csv_errors_name_the_file_and_the_line() {
        let e = csv("t,a\n0,1\n0,2\n").unwrap_err().0;
        assert_eq!(e, "data file 'd': line 3: times must be ascending");
        assert!(
            csv("t,a\n0,1\n1,x\n")
                .unwrap_err()
                .0
                .contains("line 3, column 2: 'x' is not a number")
        );
        assert!(
            csv("t,a\n0,1\n1,2,3\n")
                .unwrap_err()
                .0
                .contains("line 3: has 3 columns, the first row has 2")
        );
        assert!(csv("0\n1\n").unwrap_err().0.contains("needs a time column"));
        assert!(csv("t,a\n").unwrap_err().0.contains("no rows"));
        assert!(csv("").unwrap_err().0.contains("no rows"));
        assert!(
            DataSet::parse("d", &[0xff, 0xfe, 0x00])
                .unwrap_err()
                .0
                .contains("not text")
        );
    }

    #[test]
    fn wav_16_bit_stereo() {
        let bytes = wav_16(1000, &[vec![0, 16384, -32768, 0], vec![32767, 0, 0, 0]]);
        let d = DataSet::parse("w", &bytes).unwrap();
        assert_eq!((d.channel_count(), d.len(), d.rate_hz()), (2, 4, Some(1000.0)));
        assert_eq!(d.duration_ms(), 3.0);
        assert_eq!(d.at(1.0, 0), 0.5);
        assert_eq!(d.at(1.5, 0), -0.25);
        assert_eq!(d.at(2.0, 0), -1.0);
        assert_eq!(d.at(3.0, 0), 0.0, "the last sample");
        assert_eq!(d.at(3.1, 0), 0.0, "after it");
        assert_eq!(d.at(-0.1, 0), 0.0, "before it");
        assert!((d.at(0.0, 1) - 32767.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn wav_other_formats_and_errors() {
        // 8-bit unsigned, mono.
        let mut b = wav_16(8000, &[vec![0, 0]]);
        b[34] = 8;
        b[32] = 1;
        let data = 44;
        b.truncate(data);
        b.extend([128u8, 255, 0]);
        let n = (b.len() - 44) as u32;
        b[40..44].copy_from_slice(&n.to_le_bytes());
        let d = DataSet::parse("w8", &b).unwrap();
        assert_eq!((d.len(), d.at(0.0, 0)), (3, 0.0));
        assert!((d.at(0.125, 0) - 127.0 / 128.0).abs() < 1e-6);
        assert_eq!(d.at(0.25, 0), -1.0);
        assert!(
            DataSet::parse("x", b"RIFF\0\0\0\0WAVE")
                .unwrap_err()
                .0
                .contains("no fmt chunk")
        );
        assert!(DataSet::parse("x", b"RIFF").unwrap_err().0.contains("not a WAV"));
        let mut bad = wav_16(8000, &[vec![1]]);
        bad[20] = 2; // ADPCM
        assert!(DataSet::parse("x", &bad).unwrap_err().0.contains("unsupported format"));
    }

    #[test]
    fn base64_round_trip() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(decode_base64("").unwrap(), b"");
        assert!(decode_base64("a$").is_err());
    }
}
