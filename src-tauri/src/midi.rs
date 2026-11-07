//! 最小 SMF（Standard MIDI File）解析器：零依赖，只解析分析需要的那部分。
//!
//! 职责边界：字节 → 结构化的音符/速度/拍号/调号。不理解和声，不做量化。
//!
//! 格式要点见 `docs/midi-format-research.md`，这里落实其中最容易出错的几条：
//! - 按 chunk 长度读，不假设 `MThd` 恒为 6 字节
//! - `division` 要判 bit15（PPQ / SMPTE 两套换算）
//! - running status 遇 `FF`/`F0`/`F7` 必须清除
//! - Note On 力度 0 等同 Note Off
//! - **同一音高可以同时有多个 note-on**（采样包里很常见），所以配对用队列而不是单值
//! - 缺 note-off 的音用轨道末尾兜底关闭，并记 warning
//!
//! 全程不 panic：所有解析失败都返回 `Err(String)` 或跳过。

use std::collections::HashMap;

/// 一个音符（绝对 tick）。
#[derive(Clone, Debug)]
pub struct MidiNote {
    pub pitch: u8,
    pub start: u64,
    pub dur: u64,
    pub vel: u8,
    pub channel: u8,
    /// 来源轨道下标。
    pub track: usize,
}

impl MidiNote {
    pub fn end(&self) -> u64 {
        self.start + self.dur
    }
}

/// 时间基准。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Division {
    /// 每四分音符 tick 数。
    Ppq(u16),
    /// SMPTE：帧率（24/25/29.97/30）与每帧 tick 数。
    Smpte { fps: f64, ticks_per_frame: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct TempoChange {
    pub tick: u64,
    /// 微秒 / 四分音符。
    pub us_per_quarter: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TimeSig {
    pub tick: u64,
    pub numerator: u8,
    /// 分母的负二次幂（2 = 四分音符，3 = 八分）。
    pub denominator_pow: u8,
}

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub index: usize,
    pub name: Option<String>,
    /// 该轨用到的通道（去重、升序）。
    pub channels: Vec<u8>,
    pub notes: Vec<MidiNote>,
    /// 该轨最后一个事件的 tick（含 End of Track）。
    pub end_tick: u64,
}

impl TrackInfo {
    /// 是否只有打击乐（通道 9，即界面上的通道 10）。
    pub fn is_drum_only(&self) -> bool {
        !self.channels.is_empty() && self.channels.iter().all(|&c| c == 9)
    }

    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// 音域（最低, 最高）。
    pub fn range(&self) -> Option<(u8, u8)> {
        let mut low = u8::MAX;
        let mut high = 0u8;
        for n in &self.notes {
            low = low.min(n.pitch);
            high = high.max(n.pitch);
        }
        if self.notes.is_empty() {
            None
        } else {
            Some((low, high))
        }
    }
}

#[derive(Clone, Debug)]
pub struct Smf {
    pub format: u16,
    pub division: Division,
    pub tracks: Vec<TrackInfo>,
    pub tempos: Vec<TempoChange>,
    pub timesigs: Vec<TimeSig>,
    /// 调号：(升降号个数 -7..7, 0=大调 1=小调)。
    pub key_sig: Option<(i8, u8)>,
    pub warnings: Vec<String>,
}

impl Smf {
    /// 四分音符（一拍）对应的 tick 数。
    pub fn ppq(&self) -> f64 {
        match self.division {
            Division::Ppq(v) => v as f64,
            Division::Smpte { fps, ticks_per_frame } => fps * ticks_per_frame as f64,
        }
    }

    /// 全部轨道的音符总数。
    pub fn note_count(&self) -> usize {
        self.tracks.iter().map(|t| t.notes.len()).sum()
    }

    /// 最后一拍的 tick。
    pub fn end_tick(&self) -> u64 {
        self.tracks.iter().map(|t| t.end_tick).max().unwrap_or(0)
    }

    /// 首个拍号（缺省 4/4）。
    pub fn first_time_sig(&self) -> (u8, u8) {
        match self.timesigs.first() {
            Some(ts) => (ts.numerator, 1u8 << ts.denominator_pow),
            None => (4, 4),
        }
    }

    /// tick → 秒（逐段积分 tempo map，不是乘一个常数）。
    pub fn sec_of_tick(&self, tick: u64) -> f64 {
        match self.division {
            Division::Smpte { fps, ticks_per_frame } => {
                tick as f64 / (fps * ticks_per_frame as f64)
            }
            Division::Ppq(ppq) => {
                let ppq = ppq as f64;
                let mut sec = 0.0;
                let mut last = 0u64;
                // 默认 120 BPM
                let mut us_per_quarter = 500_000f64;
                for tc in &self.tempos {
                    if tc.tick >= tick {
                        break;
                    }
                    if tc.tick >= last {
                        sec += (tc.tick - last) as f64 * us_per_quarter / 1_000_000.0 / ppq;
                        last = tc.tick;
                        us_per_quarter = tc.us_per_quarter as f64;
                    }
                }
                sec += (tick.saturating_sub(last)) as f64 * us_per_quarter / 1_000_000.0 / ppq;
                sec
            }
        }
    }

    /// 第一个速度（BPM）；没有 tempo 事件时为 `None`（调用方应回退到默认 120 并提示）。
    pub fn first_bpm(&self) -> Option<f64> {
        self.tempos
            .first()
            .map(|t| 60_000_000.0 / t.us_per_quarter as f64)
    }

    pub fn parse(bytes: &[u8]) -> Result<Smf, String> {
        let mut r = Reader::new(bytes);
        let mut warnings: Vec<String> = Vec::new();

        // ---- 头块 ----
        let id = r.take(4).ok_or("文件太小，读不到 MThd")?;
        if id != b"MThd" {
            return Err(format!("不是标准 MIDI 文件（头块是 {:?}）", String::from_utf8_lossy(id)));
        }
        let header_len = r.u32be().ok_or("头块长度读不到")? as usize;
        let header = r.take(header_len).ok_or("头块内容不完整")?;
        if header.len() < 6 {
            return Err("头块长度不足 6 字节".to_string());
        }
        let format = be16(&header[0..2]);
        let _declared_tracks = be16(&header[2..4]);
        let division = parse_division(be16(&header[4..6]));

        let mut tracks: Vec<TrackInfo> = Vec::new();
        let mut tempos: Vec<TempoChange> = Vec::new();
        let mut timesigs: Vec<TimeSig> = Vec::new();
        let mut key_sig: Option<(i8, u8)> = None;

        // ---- 块循环 ----
        while let Some(id) = r.take(4) {
            let len = match r.u32be() {
                Some(v) => v as usize,
                None => {
                    warnings.push("块头长度缺失，停止解析".to_string());
                    break;
                }
            };
            let data = match r.take(len) {
                Some(d) => d,
                None => {
                    warnings.push(format!("块 {:?} 声明长度超过文件尾，已截断", String::from_utf8_lossy(id)));
                    r.rest()
                }
            };
            if id == b"MTrk" {
                let index = tracks.len();
                let parsed = parse_track(data, index, &mut warnings);
                // 速度和拍号可能在任何轨（format 0 常见），全部收集
                tempos.extend(parsed.tempos);
                timesigs.extend(parsed.timesigs);
                if key_sig.is_none() {
                    key_sig = parsed.key_sig;
                }
                tracks.push(parsed.track);
            } else if id != b"MThd" {
                warnings.push(format!(
                    "跳过未知块 {:?}（{} 字节）",
                    String::from_utf8_lossy(id),
                    len
                ));
            }
        }

        if tracks.is_empty() {
            return Err("文件里没有 MTrk 轨道".to_string());
        }

        tempos.sort_by_key(|t| t.tick);
        timesigs.sort_by_key(|t| t.tick);

        Ok(Smf {
            format,
            division,
            tracks,
            tempos,
            timesigs,
            key_sig,
            warnings,
        })
    }
}

fn be16(b: &[u8]) -> u16 {
    ((b[0] as u16) << 8) | b[1] as u16
}

fn parse_division(raw: u16) -> Division {
    if raw & 0x8000 != 0 {
        // 高字节是负数帧率（补码）
        let hi = ((raw >> 8) & 0xFF) as u8 as i8;
        let fps = -(hi as f64);
        Division::Smpte {
            fps: if fps == 0.0 { 30.0 } else { fps },
            ticks_per_frame: (raw & 0xFF) as u8,
        }
    } else {
        Division::Ppq(if raw == 0 { 480 } else { raw })
    }
}

struct TrackParseResult {
    track: TrackInfo,
    tempos: Vec<TempoChange>,
    timesigs: Vec<TimeSig>,
    key_sig: Option<(i8, u8)>,
}

fn parse_track(data: &[u8], index: usize, warnings: &mut Vec<String>) -> TrackParseResult {
    let mut r = Reader::new(data);
    let mut tick: u64 = 0;
    let mut running: Option<u8> = None;
    let mut notes: Vec<MidiNote> = Vec::new();
    let mut name: Option<String> = None;
    let mut channels: Vec<u8> = Vec::new();
    let mut tempos: Vec<TempoChange> = Vec::new();
    let mut timesigs: Vec<TimeSig> = Vec::new();
    let mut key_sig: Option<(i8, u8)> = None;
    // (channel, pitch) → 未关闭的 note-on 队列（同音高可叠加）
    let mut sounding: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();
    let mut truncated = false;

    loop {
        if r.eof() {
            truncated = true;
            break;
        }
        let delta = match r.vlq() {
            Some(v) => v,
            None => {
                truncated = true;
                break;
            }
        };
        tick = tick.saturating_add(delta);

        let first = match r.u8() {
            Some(b) => b,
            None => {
                truncated = true;
                break;
            }
        };

        if first == 0xFF {
            // Meta
            let kind = match r.u8() {
                Some(k) => k,
                None => break,
            };
            let len = match r.vlq() {
                Some(v) => v as usize,
                None => break,
            };
            let payload = r.take(len).unwrap_or(&[]);
            match kind {
                0x2F => break, // End of Track
                0x03 => {
                    if name.is_none() {
                        name = Some(decode_text(payload));
                    }
                }
                0x51 => {
                    if payload.len() >= 3 {
                        let us = ((payload[0] as u32) << 16)
                            | ((payload[1] as u32) << 8)
                            | payload[2] as u32;
                        if us > 0 {
                            tempos.push(TempoChange {
                                tick,
                                us_per_quarter: us,
                            });
                        }
                    }
                }
                0x58 => {
                    if payload.len() >= 2 {
                        timesigs.push(TimeSig {
                            tick,
                            numerator: payload[0],
                            denominator_pow: payload[1],
                        });
                    }
                }
                0x59 => {
                    if payload.len() >= 2 {
                        key_sig = Some((payload[0] as i8, payload[1]));
                    }
                }
                _ => {} // 其余 meta 读长度跳过（向前兼容）
            }
            running = None; // meta 会打断 running status
            continue;
        }

        if first == 0xF0 || first == 0xF7 {
            let len = match r.vlq() {
                Some(v) => v as usize,
                None => break,
            };
            r.take(len);
            running = None; // SysEx 同样打断
            continue;
        }

        // 通道事件
        let (status, d1) = if first & 0x80 != 0 {
            running = Some(first);
            match r.u8() {
                Some(v) => (first, v),
                None => {
                    truncated = true;
                    break;
                }
            }
        } else {
            match running {
                Some(s) => (s, first),
                None => {
                    warnings.push(format!("轨 {} 出现缺少状态字节的数据，已放弃该轨剩余部分", index));
                    break;
                }
            }
        };

        let channel = status & 0x0F;
        if !channels.contains(&channel) {
            channels.push(channel);
        }
        let high = status & 0xF0;

        match high {
            0x80 | 0x90 => {
                let vel = match r.u8() {
                    Some(v) => v,
                    None => {
                        truncated = true;
                        break;
                    }
                };
                if high == 0x80 || vel == 0 {
                    // Note Off（力度 0 的 Note On 等价于 Off）
                    if let Some(stack) = sounding.get_mut(&(channel, d1)) {
                        if let Some((start, v)) = stack.pop() {
                            notes.push(MidiNote {
                                pitch: d1,
                                start,
                                dur: tick.saturating_sub(start),
                                vel: v,
                                channel,
                                track: index,
                            });
                        }
                    }
                } else {
                    sounding
                        .entry((channel, d1))
                        .or_default()
                        .push((tick, vel));
                }
            }
            0xA0 | 0xB0 | 0xE0 => {
                // 后触 / 控制器 / 弯音：只需要吃掉数据字节
                if r.u8().is_none() {
                    truncated = true;
                    break;
                }
            }
            0xC0 | 0xD0 => {
                // program change / 通道后触：data 已在 d1 里
            }
            _ => {
                // 理论上不可达
                break;
            }
        }
    }

    // 未关闭的音：用轨道末尾兜底，避免丢失
    let mut leftovers = 0usize;
    for ((channel, pitch), stack) in sounding.iter() {
        for &(start, vel) in stack.iter() {
            leftovers += 1;
            notes.push(MidiNote {
                pitch: *pitch,
                start,
                dur: tick.saturating_sub(start).max(1),
                vel,
                channel: *channel,
                track: index,
            });
        }
    }
    if leftovers > 0 {
        warnings.push(format!("轨 {} 有 {} 个音符缺少 note-off，已按轨道末尾兜底", index, leftovers));
    }
    if truncated {
        warnings.push(format!("轨 {} 数据提前结束（可能缺 End of Track）", index));
    }

    notes.sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    channels.sort_unstable();
    channels.dedup();

    TrackParseResult {
        track: TrackInfo {
            index,
            name,
            channels,
            notes,
            end_tick: tick,
        },
        tempos,
        timesigs,
        key_sig,
    }
}

fn decode_text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).to_string();
    s.trim_matches('\0').trim().to_string()
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, p: 0 }
    }

    fn eof(&self) -> bool {
        self.p >= self.b.len()
    }

    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.p)?;
        self.p += 1;
        Some(v)
    }

    fn u32be(&mut self) -> Option<u32> {
        let s = self.take(4)?;
        Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.p.checked_add(n)?;
        if end > self.b.len() {
            return None;
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Some(s)
    }

    fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.p.min(self.b.len())..];
        self.p = self.b.len();
        s
    }

    /// VLQ（7 位一组，最高位表示还有后续，最多 4 字节）。
    fn vlq(&mut self) -> Option<u64> {
        let mut value: u64 = 0;
        for _ in 0..4 {
            let b = self.u8()?;
            value = (value << 7) | (b & 0x7F) as u64;
            if b & 0x80 == 0 {
                return Some(value);
            }
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_decoding() {
        let mut r = Reader::new(&[0x00]);
        assert_eq!(r.vlq(), Some(0));
        let mut r = Reader::new(&[0x7F]);
        assert_eq!(r.vlq(), Some(127));
        let mut r = Reader::new(&[0x81, 0x00]);
        assert_eq!(r.vlq(), Some(128));
        let mut r = Reader::new(&[0xFF, 0x7F]);
        assert_eq!(r.vlq(), Some(16383));
    }

    #[test]
    fn rejects_non_midi() {
        assert!(Smf::parse(b"RIFFxxxx").is_err());
        assert!(Smf::parse(b"").is_err());
    }

    #[test]
    fn division_variants() {
        assert_eq!(parse_division(480), Division::Ppq(480));
        match parse_division(0xE728) {
            // 25 fps, 40 ticks/frame
            Division::Smpte { fps, ticks_per_frame } => {
                assert!((fps - 25.0).abs() < 0.01);
                assert_eq!(ticks_per_frame, 40);
            }
            other => panic!("期望 SMPTE，实际 {:?}", other),
        }
    }

    #[test]
    fn minimal_smf_with_one_note() {
        // MThd(6) format0 ntracks1 ppq96 + MTrk: note on C4, note off, end
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&96u16.to_be_bytes());
        let mut trk: Vec<u8> = Vec::new();
        trk.extend_from_slice(&[0x00, 0x90, 60, 100]); // on
        trk.extend_from_slice(&[0x60, 0x80, 60, 0]); // off
        trk.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]); // end
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(trk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trk);

        let smf = Smf::parse(&bytes).expect("应能解析");
        assert_eq!(smf.format, 0);
        assert_eq!(smf.tracks.len(), 1);
        assert_eq!(smf.note_count(), 1);
        let n = &smf.tracks[0].notes[0];
        assert_eq!(n.pitch, 60);
        assert_eq!(n.start, 0);
        assert_eq!(n.dur, 96);
    }

    #[test]
    fn overlapping_same_pitch_is_not_lost() {
        // 同一音高连续两次 note-on（采样包常见），用队列配对
        let mut trk: Vec<u8> = Vec::new();
        trk.extend_from_slice(&[0x00, 0x90, 60, 100]);
        trk.extend_from_slice(&[0x00, 0x90, 60, 100]);
        trk.extend_from_slice(&[0x30, 0x80, 60, 0]);
        trk.extend_from_slice(&[0x30, 0x80, 60, 0]);
        trk.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&96u16.to_be_bytes());
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(trk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trk);

        let smf = Smf::parse(&bytes).expect("应能解析");
        assert_eq!(smf.note_count(), 2, "两个 note-on 都要有对应音符");
    }
}
