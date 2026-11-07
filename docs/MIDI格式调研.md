# MIDI 文件格式与 JS 读写 · 调研笔记

> 结论先说：**纯 JS 手写 SMF 完全可行，零依赖约 200 行，产物极小**。
> 本文的事实部分对照 [somascape SMF 技术规范](http://www.somascape.org/midi/tech/mfile.html) 核对，
> 并由 `scripts/smf-writer-demo.mjs` 实测验证（见 §7）。

---

## 1. 文件结构：三层嵌套

```
文件
└── MThd（头块，只能一个，必须最前）
└── MTrk × ntracks（轨块）
    └── 事件 × N ＝ 增量时间(VLQ) + 事件体
        ├── MIDI 通道事件  8n..En
        ├── SysEx  F0 / F7
        └── Meta   FF type len data
```

所有多字节整数**大端**。每块格式固定：`4 字节标识 + 4 字节长度 + 数据`。

### MThd

| 偏移 | 长度 | 内容 |
| --- | --- | --- |
| 0 | 4 | `MThd` |
| 4 | 4 | 长度，**当前恒为 6，但读的时候必须按长度读**（规范明说别假设） |
| 8 | 2 | format：0=单轨 / 1=多轨同步（tempo 放第一轨）/ 2=彼此独立的序列 |
| 10 | 2 | ntracks（format 0 必须为 1） |
| 12 | 2 | division（见下） |

### division 的两种含义（最容易被忽略的一处）

- **bit15 = 0**：ticks per quarter note（PPQ），1..32767。90% 的文件是这种，常见 480 / 960。
- **bit15 = 1**：SMPTE，高字节是**负数**帧率（-24/-25/-29/-30 的补码），低字节是每帧 tick 数。
  时间与音乐无关（跟视频同步用）。读的时候必须判断，写成 PPQ 就会把老文件的时间算错。

### MTrk

`MTrk` + 长度 + 事件流。逐条读：先读 VLQ 增量时间，再读事件体。

### 增量时间 VLQ

7 位一组，**最高位 = 1 表示还有后续字节**，最多 4 字节（上限 0x0FFFFFFF）。
例：`0x00`→0，`0x7F`→127，`0x81 0x00`→128，`0xFF 0x7F`→16383。
它表示"距上一个事件的 tick 数"，所以解码是**累加**，不是绝对时间。

### 事件体的三种开头

| 首字节 | 类型 | 说明 |
| --- | --- | --- |
| `0x80..0xEF` | 通道事件 | 高 4 位消息类型，低 4 位通道（0–15） |
| `0xF0` / `0xF7` | SysEx | `F0` 完整消息、`F7` 续包/转义；都以 VLQ 长度收尾 |
| `0xFF` | Meta | `FF type VLQ长度 数据` |

**Running status**：连续多个通道事件同状态时，后续可省略状态字节。规则要点：
只对通道事件有效；**遇到 Meta 或 SysEx 就失效**（必须先重发状态字节）。
顺手的一点是 Note On 力度 0 等价 Note Off，所以 note on/off 混排也能共用 running status。

---

## 2. 通道事件速查

`n` = 通道 0–15。文件里的通道是 0 基，界面上的"通道 10"是鼓，对应文件里 `0x99`。

| 状态 | 长度 | 数据 | 备注 |
| --- | --- | --- | --- |
| `8n` Note Off | 3 | note, velocity | 力度 0 也合法 |
| `9n` Note On | 3 | note, velocity | **velocity=0 等同 Note Off** |
| `An` Poly Pressure | 3 | note, pressure | 逐键后触 |
| `Bn` Controller | 3 | cc, value | 7=音量 10=声像 64=延音踏板 1=调制轮 |
| `Cn` Program Change | 2 | program | GM 音色表 0–127 |
| `Dn` Channel Pressure | 2 | pressure | 整通道后触 |
| `En` Pitch Bend | 3 | lsb, msb | **14 位，中心 8192**，不是 64 |

---

## 3. Meta 事件（重点几个）

格式统一 `FF type VLQ长度 数据`。

| type | 名称 | 数据 | 备注 |
| --- | --- | --- | --- |
| `00` | Sequence Number | 2 | |
| `01` `02` `03` `04` | Text / Copyright / TrackName / InstrumentName | 文本 | 编码约定乱，中文建议试 UTF-8，DAW 不一定认 |
| `05` `06` `07` | Lyric / Marker / CuePoint | 文本 | Marker 很适合存"和弦名、段落" |
| `20` | MIDI Channel Prefix | 1 | |
| `21` | MIDI Port | 1 | 多端口设备 |
| `2F` | End of Track | 0 | **必须有**，否则严格解析器拒收 |
| `51` | Set Tempo | 3 | **微秒/四分音符**，不是 BPM：`BPM = 60_000_000 / 值`。默认 120 |
| `54` | SMPTE Offset | 5 | |
| `58` | Time Signature | 4 | `nn` 分子、`dd` 分母的**负二次幂**（2=四分音符，3=八分）、`cc` 每拍 MIDI clock、`bb` 每四分音符的三十二分音符数（通常 8） |
| `59` | Key Signature | 2 | `sf` = -7..7（负数为降号数）、`mi` = 0 大调 / 1 小调。format 1 里只放第一轨 |
| `7F` | Sequencer Specific | 变长 | 厂商私有 |

未知 type 的正确处理：**读长度、跳过、保留**，不要报错——这是向前兼容的唯一途径。

---

## 4. 音高映射

- MIDI 号 0–127。`C-1 = 0`，`G9 = 127`。
- **中间 C 的编号有分歧**：somascape / JMP 记作 **C4 = 60**；Yamaha 与部分 DAW 记作 C3 = 60。
  自己写插件时**内部统一用整数 MIDI 号**，只在显示层做音名，别在数据层存音名。
- 常用区间：61 键标准键盘 36–96；88 键钢琴 21–108。
- 本仓库 `chords` 项目的后端 `note.rs` 已经有一套音名解析/拼写逻辑，可以直接复用（尤其"按调号拼写 Bb 而非 A#"）。

---

## 5. 读写实现要点（坑清单）

**写**
1. 秒 → tick 必须**取整**并保证单调；浮点误差会让 note-off 跑到 note-on 前面。
2. 同一 tick 上的顺序：**先 off 后 on**，否则同一音高会粘连（本次样例用 `end = start + BAR - 1` 错开 1 tick）。
3. 每条轨补 `FF 2F 00`。
4. 没有 `FF 51` 时播放器按 120 BPM 处理——想要别的速度必须显式写。
5. PPQ 自己定，但要知道外部工具的格值，别混用。
6. 不写 running status 是**完全合法**的，文件大一点但不会错——先正确再优化。
7. 文本 meta 长度上限实际很宽松（VLQ），但别指望中文在所有 DAW 里正确显示。

**读**
1. 按 chunk 长度读，不假设 `MThd` 恒 6 字节。
2. 判 division 的 bit15，SMPTE 走另一套换算。
3. 维护 running status，遇到 `FF`/`F0`/`F7` 置空。
4. `FF 2F 00` 之后的内容忽略。
5. 容忍未知 meta / 未知 chunk（跳过）。
6. 累积 delta 得到绝对 tick，再用 tempo map 转秒（**多层速度变化要逐段积分**，不是乘一个常数）。

---

## 6. JS 生态对照（2026-02 于 npm registry 实查）

| 方案 | 版本 | 依赖 | 体积 | 定位 |
| --- | --- | --- | --- | --- |
| **手写** | — | 0 | ~200 行 | 完全可控，无供应链风险，只支持自己需要的子集 |
| `midi-file` | 1.2.4 | **无** | 46 KB | 解析 + 写入，JSON 中间表示，running status 处理完善，最靠谱的底座 |
| `@tonejs/midi` | 2.0.28 | `midi-file`, `array-flatten` | 281 KB | 高层封装：Note 对象、音名、秒、PPQ 换算 |
| `midi-writer-js` | 3.2.1 | `@tonaljs/midi` | 326 KB | 只写不读，链式 API，适合快速生成 |

**取舍**：要"读 + 改 + 写"闭环，`midi-file` 是性价比最高的依赖；要做 DSH 插件且希望零依赖可审计，手写更好——格式本身就是几十行编码逻辑，风险集中在自己能测的部分。

---

## 7. 实测验证（`scripts/smf-writer-demo.mjs`）

手写器生成 4 小节 I–vi–ii–V（Cmaj7 · Am7 · Dm7 · G7），PPQ 480、120 BPM、3 轨（conductor / keys / bass），
共 **306 字节**。随后用**第三方解析器**（VST3 工作室的 MIDI 解析器，非本人代码）读回：

| 期望 | 解析器读回 |
| --- | --- |
| PPQ 480 · 120 BPM | `ppq=480, 120 BPM` ✓ |
| 总长 4 小节 = 8.000 s | `8.00s` ✓ |
| 音符 20 个（和弦 16 + 贝斯 4） | `音符 20 个，轨道 3 条` ✓ |
| C 和弦 C4/E4/G4/B4 | `60 / 64 / 67 / 71` ✓（C4=60 约定成立） |
| 每和弦 1 小节 = 2.000 s | `dur=1.999s` ✓（故意少 1 tick 防粘连） |
| 力度 80 → 0.63 | `vel=0.63` ✓（80/127=0.630） |

**结论：格式理解正确，读写可行，且产物可直接喂给采样器/合成器渲染成音频。**

---

## 8. 对 `chords` 项目的落地价值

本项目后端已经把"和弦音集合 + 调性 + 走向"算得很干净，MIDI 正好是它缺的**输出格式**：

1. **和弦 → MIDI**：`progression.rs` 的罗马数字走向直接展开成声部（voicing / 转位 / 低音），
   走一次 `writeSmf` 就能导出可播放、可拖进 DAW 的文件。
2. **MIDI → 和弦**：读回 `.mid`，按时间切片聚合成和弦音集合，喂给现有 `analysis.rs`，
   等于"从一段 MIDI 里反推调性与走向"。
3. **双向校验**：写出去再读回来，比对音高/时值，是对分析内核最好的一种端到端测试。
4. 若做 DSH 插件，工具面建议：`midi_write`（从音符/走向生成）、`midi_read`（解析成结构）、
   `midi_transform`（移调/量化/声部重排）、`midi_analyze`（接本项目分析内核）。
