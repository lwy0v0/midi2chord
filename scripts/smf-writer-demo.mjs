// 零依赖 SMF (Standard MIDI File) 写入器 —— 可行性验证样例
// 只做格式 1：一条 tempo/conductor 轨 + 任意条音符轨。
// 运行：node scripts/smf-writer-demo.mjs

import { writeFileSync } from 'node:fs';

// ---------- 基础编码 ----------

/** 变量长度量（VLQ）：7 位一组，最高位表示"后面还有字节"，最多 4 字节 */
function vlq(value) {
  if (!Number.isInteger(value) || value < 0) throw new Error(`VLQ 需要非负整数，收到 ${value}`);
  if (value > 0x0fffffff) throw new Error(`VLQ 超上限 0x0FFFFFFF，收到 ${value}`);
  const bytes = [value & 0x7f];
  let v = value >>> 7;
  while (v > 0) {
    bytes.unshift((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  return bytes;
}

/** 大端 u16 / u32 */
const u16 = (n) => [(n >> 8) & 0xff, n & 0xff];
const u32 = (n) => [(n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];

/** 块：4 字节标识 + 4 字节长度 + 数据 */
function chunk(id, data) {
  const idBytes = [...id].map((c) => c.charCodeAt(0));
  return [...idBytes, ...u32(data.length), ...data];
}

/** 音名 → MIDI 号，约定 C4 = 60（中间 C，与 somascape/JMP 一致），C-1 = 0 */
export function noteToMidi(name) {
  const m = /^([A-Ga-g])([#b]{0,2})(-?\d)$/.exec(name.trim());
  if (!m) throw new Error(`无法解析音名：${name}`);
  const base = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1].toUpperCase()];
  const acc = [...m[2]].reduce((a, c) => a + (c === '#' ? 1 : -1), 0);
  const midi = (Number(m[3]) + 1) * 12 + base + acc;
  if (midi < 0 || midi > 127) throw new Error(`音名超出 0..127：${name} → ${midi}`);
  return midi;
}

/** 文本 meta 的上限是 127 字节（长度用 VLQ，但规范里文本类约定短） */
const textBytes = (s) => [...Buffer.from(s, 'utf8')];

// ---------- 事件构造（都是"数据字节"，delta 在序列化时再加） ----------

const ev = {
  trackName: (text) => ({ type: 'meta', meta: 0x03, data: textBytes(text) }),
  tempo: (bpm) => ({ type: 'meta', meta: 0x51, data: u32(Math.round(60_000_000 / bpm)).slice(1) }), // 3 字节微秒/四分音符
  timeSig: (num, denPow2, clocksPerClick = 24, thirtySeconds = 8) => ({
    type: 'meta', meta: 0x58, data: [num, denPow2, clocksPerClick, thirtySeconds],
  }),
  keySig: (sharps, minor = 0) => ({ type: 'meta', meta: 0x59, data: [sharps & 0xff, minor ? 1 : 0] }),
  marker: (text) => ({ type: 'meta', meta: 0x06, data: textBytes(text) }),
  program: (ch, program) => ({ type: 'channel', status: 0xc0 | ch, data: [program] }),
  noteOn: (ch, note, vel) => ({ type: 'channel', status: 0x90 | ch, data: [note, vel] }),
  noteOff: (ch, note, vel = 64) => ({ type: 'channel', status: 0x80 | ch, data: [note, vel] }),
  cc: (ch, controller, value) => ({ type: 'channel', status: 0xb0 | ch, data: [controller, value] }),
  // 弯音：14 位，中心 8192
  pitchBend: (ch, value) => {
    const v = Math.max(0, Math.min(16383, Math.round(value)));
    return { type: 'channel', status: 0xe0 | ch, data: [v & 0x7f, (v >> 7) & 0x7f] };
  },
  endOfTrack: () => ({ type: 'meta', meta: 0x2f, data: [] }),
};

function serializeEvent(e) {
  if (e.type === 'meta') return [0xff, e.meta, ...vlq(e.data.length), ...e.data];
  return [e.status, ...e.data];
}

/** 把 [{tick, event}] 按 tick 稳定排序后写成 MTrk 块（不启用 running status，最直白也最好调） */
function trackChunk(events, ppq) {
  const sorted = events
    .map((e, i) => ({ ...e, i }))
    .sort((a, b) => a.tick - b.tick || a.i - b.i); // 同 tick 保持插入顺序：note-off 在前更安全，由调用方保证
  const body = [];
  let last = 0;
  for (const { tick, event } of sorted) {
    if (tick < last) throw new Error(`事件 tick 未单调递增：${tick} < ${last}`);
    body.push(...vlq(tick - last), ...serializeEvent(event));
    last = tick;
  }
  // 规范要求每条轨以 End of Track 结束；没有就补一个
  if (!sorted.some((e) => e.type === 'meta' && e.event.meta === 0x2f)) {
    body.push(...vlq(0), 0xff, 0x2f, 0x00);
  }
  return chunk('MTrk', body);
}

/** 组装整个文件 */
export function writeSmf({ ppq = 480, tracks }) {
  const header = chunk('MThd', [...u16(1), ...u16(tracks.length), ...u16(ppq)]);
  return Buffer.from([...header, ...tracks.flatMap((t) => trackChunk(t, ppq))]);
}

// ---------- 样例：4 小节 I–vi–ii–V ----------

const PPQ = 480;
const BEAT = PPQ;
const BAR = 4 * BEAT;

const chords = [
  { bar: 0, name: 'Cmaj7', notes: ['C4', 'E4', 'G4', 'B4'], bass: 'C2' },
  { bar: 1, name: 'Am7', notes: ['A3', 'C4', 'E4', 'G4'], bass: 'A1' },
  { bar: 2, name: 'Dm7', notes: ['D4', 'F4', 'A4', 'C5'], bass: 'D2' },
  { bar: 3, name: 'G7', notes: ['G3', 'B3', 'D4', 'F4'], bass: 'G1' },
];

const conductor = [
  { tick: 0, event: ev.trackName('conductor') },
  { tick: 0, event: ev.tempo(120) },
  { tick: 0, event: ev.timeSig(4, 2) },
  { tick: 0, event: ev.keySig(0, 0) }, // C 大调，0 个升降号
  ...chords.map((c) => ({ tick: c.bar * BAR, event: ev.marker(c.name) })),
];

const keys = [{ tick: 0, event: ev.trackName('keys') }, { tick: 0, event: ev.program(0, 0) }];
const bass = [{ tick: 0, event: ev.trackName('bass') }, { tick: 0, event: ev.program(1, 32) }];

for (const c of chords) {
  const start = c.bar * BAR;
  const end = start + BAR - 1; // 末尾留 1 tick，避免和下一个和弦的 note-on 撞在同一 tick
  for (const n of c.notes) {
    const midi = noteToMidi(n);
    keys.push({ tick: start, event: ev.noteOn(0, midi, 80) });
    keys.push({ tick: end, event: ev.noteOff(0, midi) });
  }
  const b = noteToMidi(c.bass);
  bass.push({ tick: start, event: ev.noteOn(1, b, 96) });
  bass.push({ tick: end, event: ev.noteOff(1, b) });
}

const bytes = writeSmf({ ppq: PPQ, tracks: [conductor, keys, bass] });
const out = new URL('./demo-progression.mid', import.meta.url);
writeFileSync(out, bytes);

console.log(`已写出 ${bytes.length} 字节 → ${out.pathname}`);
console.log(
  '期望：4 小节 · 120bpm · PPQ 480 · 每小节 1920 tick（=2.0 秒）· 和弦 16 个音符 + 贝斯 4 个',
);
