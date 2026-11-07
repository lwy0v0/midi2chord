# 和弦与走向分析器

两条入口：**输入一串和弦音**，分析它属于哪个调、每个和弦在调里是什么角色；
或者**读进一个 MIDI 文件**（采样包里的 loop），提取它的调性、和弦进行与和声节奏。

技术栈：Vue 3 + TypeScript + Naive UI（前端）· Rust（分析内核）· Tauri 2（桌面外壳）。

## 功能

### 入口一：键盘输入的和弦序列

- **和弦识别**：全音数模板匹配，支持重复音去重、省略音（`C7(no5)`）、2 个音的强力和弦与音程、
  转位斜杠低音（`C/E`），并对同一组音给出多个候选解释。
- **调性判定**：24 个候选调逐个打分排序；自然大小调 + 和声/旋律小调的升六、升七级；
  主和弦位置、终止式、走向模板、调号复杂度都参与打分。
- **走向解释**：罗马数字、功能分组（T/S/D）、终止式（完全 / 变格 / 阻碍 / 半终止 / ii–V–I）、
  经典走向模板（流行四和弦、50 年代、卡农、安达卢西亚、12 小节布鲁斯…）。
- **调外解释**：副属和弦 `V7/vi`、同主音借用 `bVII`、那不勒斯 `bII`、皮卡迪三度，而不是简单报「找不到调」。
- **按调号拼写**：C 大调里的降七级显示为 `Bb` 而不是 `A#`；小调升七级显示为 `G#`。
- **界面**：暗色主题、中文语言包、错误直接定位到具体和弦的具体音位。

### 入口二：MIDI 文件（Loop 提取）

- **读文件**：零依赖的 MIDI 解析（变速、两种时间基准、状态省略、重叠同音、缺 note-off 都有兜底）。
- **分声部**：剔除鼓轨；按"时值 × 是否成簇"判别和弦音与旋律音（采样包的典型形态是"和弦长、旋律短"）。
- **提取调性与和弦**：**不预设窗口**——枚举 24 个调 × 调内 7 个级数 × 三/七和弦，
  用动态规划求整条最优序列，调性和和弦一起解出来（一小节 1 个、2 个、4 个和弦都能自己定）。
- **给结论也给出处**：每段带置信度、被排除的音、代表音（每个和弦音只取时值最长的那一个）；
  **每个音符**都标了角色（根/三/五/七/延伸/旋律/段外），可直接用于钢琴卷帘窗上色。
- **会拒答**：单声部素材（只有旋律或贝斯）只给调性，不硬凑和弦名。

## 模块划分

后端（`src-tauri/src/`，单向依赖、无环）：

| 模块 | 职责 |
| --- | --- |
| `note.rs` | 音高类、音名解析与拼写、位掩码工具；不 panic、不理解调性 |
| `key.rs` | 24 个候选调、音阶掩码、调号与按调号拼写 |
| `chord.rs` | 和弦模板、识别打分、省略音、转位、多解候选 |
| `progression.rs` | 调内角色、罗马数字、功能、终止式、走向模板、调外解释 |
| `scoring.rs` | 24 调打分排序、多解择优 |
| `analysis.rs` | 输入校验、编排、对外 JSON 契约、Tauri 命令 |

前端（`src/`）：

| 目录 | 职责 |
| --- | --- |
| `types.ts` | 与后端一一对应的 JSON 契约类型（含 MIDI 分析的全部结构） |
| `api/analyze.ts` | 键盘输入入口，唯一与 `invoke` 打交道的地方，错误归一化 |
| `api/midi.ts` | MIDI 入口：传字节、拿结果，另附角色配色与展示辅助 |
| `composables/` | `useChordGrid`（输入网格与校验）、`useChordAnalysis`（分析流程与错误定位） |
| `components/` | 纯展示组件：尺寸控制、输入网格、结果面板、候选调卡片、和弦chip |
| `utils/format.ts` | 展示格式化 |
| `constants/notes.ts` | 音名选项与取值范围 |

## 对外契约

### 入口一：`analyze`（和弦序列）

返回 `{ keys, recognized, warnings, total }`。**旧字段保持不变**（`keys[].key`、
`keys[].chords[].chord/name/level`），其余为新增字段：

- `keys[].{isMajor, mode, displayName, fullName, score, confidence, rank, matched, explained,
  total, romans, cadences, templates, reasons, scaleNotes, accidentals}`
- `keys[].chords[].{symbol, root, bass, quality, qualityLabel, suffix, kind, label, roman, degree,
  function, functionLabel, inScale, category, explanation, tags, alternatives, notes, missing,
  extras, warning}`

校验失败时返回结构化的 `AnalyzeError { code, message, row, col }`（行列从 0 起），
前端据此把错误高亮到具体格子；不会 panic。

### 入口二：`analyze_midi`（MIDI 字节）

返回一个 `LoopAnalysis`（字段全部 camelCase）：

- **文件与时间**：`bpm`、`bpmExplicit`（false 表示文件没写速度、用的是默认 120）、
  `beatsPerBar`、`bars`、`pickupBeats`（弱起位置）
- **调性**：`keyName`、`keyConfidence`、`keyCandidates`、`keyFromPitches`（仅按音级分布得出的调性，用于互校）
- **和弦段**：`segments[]`，每段含 `startBeat` / `lenBeats` / `bar` / `beatInBar` /
  `chord`（`root`、`triadSuffix`、`seventh`、`bass`、`slash`、`symbol`、`confidence`）/
  `chordTones` / `excluded` / `voicing[]`（代表音）
- **摘要**：`progression`、`romans`
- **音符级标注**：`notes[]`，每音含 `pitch` / `name` / `startBeat` / `durBeats` / `velocity` /
  `segment` / `role` / `weight` / `clustered` —— 钢琴卷帘窗直接按 `role` 上色
- **判断与提示**：`layers`（声部识别的结果）、`warnings`、`singleLine`（单声部时不出和弦段）

失败时同样返回 `AnalyzeError`，`code` 取 `emptyFile` / `midiParse` / `noNotes`。

## 打分模型

1. 每个和弦按与调的关系取贴合度：调内 1.0、和声小调 0.75、副属 0.35、同主音借用 0.25、
   那不勒斯 0.2、调外 0；首尾和弦权重加倍。
2. 主和弦加成：出现主和弦 +0.15、以主和弦开头 +0.2、以主和弦结尾 +0.25；
   **完全没出现主和弦 −0.3**（区分关系大小调的关键）。
3. 终止式与走向模板小幅加成，调号越复杂扣分越多。

`score` 是包含上述加成的综合排序分；`confidence` 只是第 1 项的加权平均百分比。

> 以上是**入口一**（和弦序列）的打分。入口二（MIDI）用的是另一套：全局解码的代价函数，
> 见《Loop 和声提取可行性分析》§9.5。

## 可复用组件

`src/components/PianoRoll.vue` —— **钢琴卷帘窗**，独立、零业务依赖、Canvas 渲染（几千音符不掉帧），样式用 scoped 自带。
只认 `pitch / start / duration` 三个必填字段，换个音乐应用可以整份复制走。

用法与完整 API 见 [钢琴卷帘窗组件](docs/钢琴卷帘窗组件.md)。

## 开发

```bash
npm install
npm run tauri dev     # 桌面应用（必须走 Tauri，浏览器里 invoke 不可用）
npm run typecheck     # vue-tsc 类型检查
npm run build         # 前端产物
cd src-tauri && cargo test    # 内核单元测试（56 个）
cd src-tauri && cargo clippy --all-targets

# 批量跑 MIDI 语料（打印调性、逐段和弦、代表音、音符角色）
cd src-tauri && cargo run --example probe
cd src-tauri && cargo run --example probe -- ../testdata/midi/01-柱式和弦/xxx.mid
```

> 如果 `src-tauri/target` 不可写（某些沙箱环境），把构建目录指到工作区内：
> `$env:CARGO_TARGET_DIR='D:\data\vue\chords\.cargo-target'`（已在 `.gitignore` 里排除）。

> `auto-imports.d.ts` 与 `components.d.ts` 由 unplugin 自动生成，类型检查需要它们存在
> （首次 clone 后先跑一次 `npm run build` 或 `npm run dev`）。

## 输入约定

- 行内第一个音按约定视为**低音**，用于识别转位与斜杠和弦。
- 同一个和弦里重复选择同一个音会自动去重；少于 2 个不同的音会被拦下并提示。

## 文档

| 文档 | 内容 |
| --- | --- |
| [Loop 和声提取可行性分析](docs/Loop和声提取可行性分析.md) | 能不能做、风险在哪、工作量与分期 |
| [前端模块地图](docs/前端模块地图.md) | **改什么该动哪个文件**（模块边界与依赖方向） |
| [钢琴卷帘窗组件](docs/钢琴卷帘窗组件.md) | **可复用组件的 API 与移植说明** |
| [同类项目调研](docs/同类项目调研.md) | 开源项目怎么做的、哪些设计可以抄 |
| [真值标注规范](docs/真值标注规范.md) | 手工标注怎么标（回归测试的基准） |
| [结果契约与展示设计](docs/结果契约与展示设计.md) | 输出结构、界面怎么摆、用户怎么纠正 |
| [MIDI 写作手法清单](docs/MIDI写作手法清单.md) | 采样包里的 MIDI 实际长什么样、有哪些坑 |
| [和弦种类与处理方法清单](docs/和弦种类与处理方法清单.md) | 和弦种类与和声处理手法的全量清单 |
| [和弦识别与MIDI扩展调研](docs/和弦识别与MIDI扩展调研.md) | 现有内核的实测缺口与实施分期 |
| [MIDI 格式调研](docs/MIDI格式调研.md) | 文件字节级格式与读写坑 |

测试语料与实测数据：[testdata/midi/语料清单.md](testdata/midi/语料清单.md)
