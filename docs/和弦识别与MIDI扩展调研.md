# 和弦识别扩展 + MIDI 输入 · 调研

> 目的：在动手之前先摸清三件事——**现有内核到底能做什么、缺口在哪（有实测）、业界怎么解**。
> 本文所有"实测"数据来自 `scripts/chord-baseline-probe.rs`（`rustc` 直编，不经过 cargo，见 §5.3）。

---

## TL;DR

1. **"音集合正确"时现有内核相当准**：三和弦 / 七和弦 / 九和弦 / 十三和弦这些标准语汇基本全中，
   模板匹配 + 省略音 + 候选排序这套设计是对的，不需要推翻。
2. **缺口是三类系统性的**，不是"模板少几个"：① 无根音 / shell voicing；② 外音（装饰音）导致的过度解释与根音跳变；③ 语汇层（等音拼写、增六、四度叠置）。
3. **MIDI 模式的本质问题是"分段"，不是"识别"**。逐段独立识别一定会崩——实测把 C9 的无根音 voicing 认成 `Em7b5`，把 ii–V–I 的 shell voicing 整条判成 **Bb 大调**。
4. 成熟解法是 **Temperley–Sleator 那条路**：不预先切分，而是"每段时间内所有还在响的音"用一个全局最优（动态规划 / HMM）同时决定**边界**和**和弦**。`music21.chordify` 是它的对立面（全切），会过度分割，只适合做可视化不适合做分析。
5. 落地建议：Rust 侧零依赖加 `midi.rs`（SMF 解析）+ `rhythm.rs`（网格/分段/解码），把 `chord.rs` 从"给答案"改成"枚举候选 + 带分"，**旧契约不动**。

---

## 1. 现状盘点

### 1.1 代码级能力与边界

| 模块 | 现在能做 | 边界（改 MIDI 时会撞上的） |
| --- | --- | --- |
| `note.rs` | 音高类 0..11、音名解析、`SpelledNote`（按调号拼写） | **只有音高类，没有八度/音区**；`PitchMask` 是 12 位 |
| `key.rs` | 24 个候选调、自然/和声/旋律小调掩码、按调号拼写 | 只表达"单一全局调"，没有"局部离调/临时主音"的位置概念 |
| `chord.rs` | 55 个模板、省略音（五音/延伸音/三音）、转位、多解候选（最多 4 个） | ① 根音**必须**来自输入音；② 只返回 top-4、且分数是内部 `i32` 不外露；③ 精确匹配优先，`extras` 惩罚固定 |
| `progression.rs` | 罗马数字、功能、终止式、走向模板、副属/借用/那不勒斯解释 | 全部是"给定一个调"下的单和弦解释；调外只给一句"未找到常规解释" |
| `scoring.rs` | 24 调打分排序、多解择优（按上下文分类） | 全局单调度；首尾加权；无"进行"层面的转移先验 |
| `analysis.rs` | 校验 → 识别 → 打分 → JSON 契约，行列级错误定位 | 输入是 `Vec<Vec<String>>`（每个和弦一组音名），**没有时间维度** |

### 1.2 实测基线

**A. 单组音识别（25 例，节选）**

| 输入音 | 现有输出 | 评 |
| --- | --- | --- |
| `C E G` / `G B D F` / `C Eb G Bb` / `C Eb Gb Bb` / `C Eb Gb A` | `C` `G7` `Cm7` `Cm7b5` `Cdim7` | ✅ 标准语汇全对 |
| `C E G B D` / `C E G D` / `C E G A` | `Cmaj9` `Cadd9` `C6` | ✅ |
| `C E G B F#` | `Cmaj7#11` | ✅ |
| `C E G Bb Db Ab`（7b9b13） | `C7b9` + **警告"含结构外音 G#"** | ⚠️ 缺 `7b9b13` 模板 |
| `C E G Bb F# A`（13#11） | `C7add13` + 外音 F# | ⚠️ 缺 `13#11` 模板 |
| `C E G Bb Db Eb`（alt） | `C7b9`（丢掉 #9） | ⚠️ 缺 `7alt` |
| `E G Bb D`（**C9 的无根音 voicing**） | `Em7b5` | ❌ 爵士钢琴左手最常见的形态之一 |
| `E G B`（**Cmaj7 的无根音**） | `Em` | ❌ 差三度级 |
| `E Bb`（shell 3-7） | `Eb5`（增四度音程） | ❌ 完全失去和弦意义 |
| `Ab C F#` / `Ab C Eb F#`（意/德增六） | `G#7(no5)` / `G#7` | ❌ 等音重写，丢掉"增六→属"的功能含义 |
| `C F Bb Eb`（四度叠置） / `D G C F A`（So What） | `F7sus4` / `F6/9` | ⚠️ 结果不算错，但不是作曲者想的东西 |
| `D C E G`（`C/D`） | `Cadd9` | ⚠️ 把斜杠低音吞成和弦音（`Cadd9/D` 的语义不同） |

> 这 25 个用例的分布：**13 个 ✅ / 7 个 ⚠️ / 5 个 ❌**，而 5 个 ❌ 全部落在
> **无根音（3 个）** 与 **增六族（2 个）** 这两类上——不是"模板不够多"的随机失败。

**B. 一个外音混进来（最要命的失败模式）**

| 输入 | 输出 | 应然 |
| --- | --- | --- |
| `C E G B` → `+ D`（经过音） | `Cmaj9` | 还是 `Cmaj7`，D 是外音 |
| `C E G` → `+ F#`（辅助音） | `C` + 警告外音 | ✅ 这一类是对的 |
| `C E G` → `+ Db`（倚音） | **`C#dimM7`** | 还是 `C`，Db 是外音 |

> 关键观察：**外音容忍在 3 音和弦上会失效**。因为任意 4 个音几乎总能找到一个"精确匹配"的模板（`C Db E G` 恰好 = `C#dimM7`），而现有打分里"精确匹配"永远压过"3 音 + 1 个外音"。**这不是参数没调好，是缺少语境**：没有前后和弦、拍位、时值，就无法判断 Db 是装饰音还是和弦音。

**C. 序列级（调性判定）**

| 序列 | 现有输出 |
| --- | --- |
| `C Am Dm G` | C 大调，`I vi ii V` ✅ |
| `C A7 Dm7 G7` | C 大调，`I V7/ii ii7 V7` ✅ 副属解释成立 |
| `C Ab G`（bVI 借用） | C 大调，`I bVI V` ✅ |
| **`F C Eb` / `F Bb D` / `E Bb D`**（ii–V–I 的无根音 shell） | **Bb 大调**，`V7(no3) I V7/vii` ❌ |
| `Cadd9` → `C`（装饰音导致的错认） | C 大调，`Iadd9 I` — 调性没错，但和弦名错了 |

**D. 单和弦在 C 大调里的角色**

| 输入 | 判定 |
| --- | --- |
| `A C# E G` | `V7/ii`，secondaryDominant ✅ |
| `Ab C Eb` | `bVI`，modalMixture ✅ |
| `Db F Ab` | `bII`，neapolitan ✅ |
| `E G Bb D` | `iiiø7`，**chromatic**「含调外音 A#」 ❌（它其实是 C9） |

### 1.3 五类缺口（从实测归纳）

| 编号 | 缺口 | 本质 |
| --- | --- | --- |
| **G1** | 无根音 / shell / rootless voicing | 根音**不在**音集合里，现在架构上不可能表达 |
| **G2** | 外音与装饰音 | 缺"时间 + 语境"来判定谁是和弦音 |
| **G3** | 语汇层：等音拼写、增六、四度叠置 | 是"同一音集合的多种功能解释"，不是模板问题 |
| **G4** | 延伸音模板缺失 | 纯枚举工作量（`13#11`、`7b9b13`、`7alt`、`m11`、`add#11`、`maj7#5add9`…） |
| **G5** | 没有时间与声部 | 数据模型只有音高类，MIDI 的音区/声部/时长/力度全丢了 |

**结论：G4 是体力活，G1/G2/G3 是设计问题，G5 是前置条件。**

---

## 2. 需求一：更智能地识别更多和弦

三条**互不重叠**的升级线，建议分三次提交，各自可独立测。

### 2.1 线 L1：模板扩容（纯数据，风险最低）

按"爵士/流行实际用到的语汇"补齐，优先级从高到低：

- **变化属和弦**：`7alt`(b9#9#5/b13 的一组)、`7b9b13`、`7#5b9`、`7#9b13`、`13b9`、`13#11`、`7sus4b9`
- **小和弦延伸**：`m11`（现有）、`m9`（现有）、`m(maj9)`、`m6/9`、`m7add11`
- **大和弦延伸**：`maj9#11`、`maj7add13`、`6/9`（现有）、`add#11`、`addb9`（少见）
- **挂留复合**：`7sus4`（现有）、`9sus4`（现有）、`sus2sus4`、`7sus4(no5)`
- **四度/现代**：quartal（`0 5 10`、`0 5 10 15`）、`So What`（`0 5 10 15 19` → 音高类 `0 3 5 7 10`）
- **无根音形态**（配合 L2）
- **增六族**（配合 L3）：意 `0 4 10`、法 `0 4 6 10`、德 `0 4 7 10`（全部相对降六级）

> 实现提醒：模板变多以后，`interpretations()` 里 `root_index * 16` 的排序键仍要保证"常用和弦优先"，
> 否则 `C E G` 可能被某个花哨的 6 音模板抢走。建议把 `TEMPLATES` 的顺序当**先验分**用，而不是排序键。

### 2.2 线 L2：根音解耦（解决 G1）

现在 `chord.rs` 里根音只能从输入音里挑（`for &root in unique`）。改成三类候选根音：

```text
候选根音 = 输入音（现有）
         ∪ { 隐式根音 | 输入音里存在至少 3 个音属于该根音的某个模板 }
         ∪ { 低音上方三度/五度推导出的根音 | 仅在 shell voicing 判定时启用 }
```

配套改动：

1. `Chord` 增加 `root_source: Input | Inferred`（是否"虚拟根音"）；
2. 虚拟根音要付**额外代价** `COST_INFERRED_ROOT`，但**低于**"换一个根音去做精确匹配"的收益——
   现在 `E G Bb D → Em7b5`（精确 4 音）压过 `C9(no root)`（命中 4 音、1 个缺根音），就是因为缺这个机制；
3. 后缀要能表达"根音未弹"：`C9(shell)` 或 `C9(no1)`，别让用户以为谱面上真有 C；
4. 判定 shell voicing 的强特征：**最低音是根音的三度或七度**（爵士左手 3-7 / 7-3）、音数 ≤ 4、低音区（< C4）。

> 这条线做完，实测里 G1 的五个失败例（`E G Bb D`、`E G B`、`E Bb`、shell ii–V–I）应该全部转正。

### 2.3 线 L3：语汇层（解决 G3）——**别在 `chord.rs` 里做**

同一个音集合有多种"拼写 + 功能"解释，属于调性/语境层的事：

| 音集合 | 解释 A | 解释 B | 判据 |
| --- | --- | --- | --- |
| `Ab C Eb F#` | `Ab7(no5)` / `G#7` | **德增六** | 后续是否接 `G`(V)：接则增六 |
| `C E G Bb D F#` | `D9` | `C13#11` | 低音是谁 + 前后调性 |
| `E G Bb D` | `Em7b5` | `C9` | 前后和弦是不是 ii–V–I |
| `C F Bb Eb` | `F7sus4` | `C 四度叠置` | 低音 + 语境 |

**建议做法**：`chord.rs` 只输出"音集合 + 若干候选（根音/性质）"，新增一个**解释层** `interpret.rs`：

```rust
fn explain(key: &Key, ctx: &[Chord], i: usize) -> Vec<Explanation>
// 按优先级产出：diatonic > secondary dominant > mixture > neapolitan
// > augmented sixth > tritone sub > modal/quartal > polychord > chromatic(兜底)
```

顺带把**等音拼写**一起解决：`Ab C F#` 现在拼成 `G#`，是因为 `Note::name()` 用升号表。
`key.spell_with()` 已经支持按调号拼写，只要 `Chord::symbol_in()` 走的是它就行——需要把
解释层的"推荐拼写方向"（降/升/字母级数）传下来。

### 2.4 参考实现：标签语法与评估基线

- **Harte 标签语法**（`C:maj7(b9,#11)/5`）是 MIR 事实标准，被 `mir_eval` 采用。
  其价值不在给人看，而在于**把"根音 / 性质 / 附加音 / 低音"四段拆开**，天然支持"和弦名不同但内容等价"的比较。
  建议内部就用这个四元组（我们已有 `root/suffix/bass`，补 `added`、`omitted` 两列即可）。
- `mir_eval.chord` 定义了 **root / majmin / triads / tetrads / sevenths / mirex** 等子字母表比较，
  以及 `overseg`/`underseg`/`seg`（专门衡量"分段边界对不对"）。
  → 这正好是本项目要的验收工具，见 §5。

---

## 3. 需求二：MIDI 模式

### 3.1 MIDI 比"一组音"多出来四样东西

| 新增维度 | 能解决什么 | 现在有没有 |
| --- | --- | --- |
| **音区（八度）** | 谁是低音、谁是旋律、谁在和弦中段 | ❌ 只有 pitch class |
| **时间（onset/duration）** | 琶音聚合、装饰音判定、和声节奏 | ❌ |
| **力度** | 装饰音通常明显偏轻 | ❌ |
| **轨道/通道** | 剔除鼓轨、把旋律与伴奏分开 | ❌ |

> `midi_analysis`（DSH 的 MIDI 插件）在跑现有的 `demo-progression.mid` 时给的两条提示，正好印证：
> "每小节只有一个和弦猜测时，半小节变化会被抹平"、"多轨混在一起分析会把伴奏与旋律叠在一起"。

### 3.2 四类演出技巧：现象 → 判据 → 处理

| 技巧 | MIDI 里的信号 | 判据（可实现的启发式） | 处理 |
| --- | --- | --- | --- |
| **琶音 / 分解和弦** | 同一和弦的音在时间上依次出现（相差 < 1 拍），整体落在 1–2 拍内 | 相邻 onset 间隔小 + 音高集合在同一模板下命中率单调上升 + 无新低音 | 归入同一段（不预先分组，见 §3.5） |
| **装饰音 / 经过音 / 辅助音** | 时值极短（< 1/8 拍）、弱拍、与相邻音级进、力度偏弱 | "外音检验"：它不是任何候选和弦的模板音，且**后一个音级进解决**到模板音 | 标为 NCT（非和弦音），不参与和弦判定，但仍显示 |
| **离调 / 临时主音化** | 连续 2–4 个和弦落在"某个别的调"里，之后回到本调 | 滑窗调性 + "V/x → x"配对检验 | 输出"局部临时主音"标注，而不是判成"调外音" |
| **一小节两个柱式和弦** | 小节中间出现低音跳变 + 全部音重新起音 | 分段边界候选 + 强拍先验（Temperley 规则 c） | 让解码器决定边界，别用固定网格硬切 |

### 3.3 为什么"先切分再识别"一定会崩（实测证据）

把 `C9` 的无根音 voicing 摊成一段一段（爵士左手最常见的弹法），逐段喂现有内核：

```text
E G Bb D          → Em7b5     （差一个和弦功能）
E Bb              → Eb5       （只剩音程）
F C Eb / F Bb D / E Bb D  → 整条判成 Bb 大调
```

**根因有两个，缺一不可**：

1. 段内音集合本身就不完整（无根音、省略音）→ 需要 §2.2 的根音解耦；
2. 段边界是"猜"的，而不是"解"出来的 → 需要 §3.5 的全局解码。

`music21.chordify` 的做法是"任何事件变化就切一刀"（salami slicing），它会产出大量假和弦——
官方文档自己举了例子：巴赫众赞歌里被切出来 4 个属七和弦，其中 3 个是弱拍经过音造成的。
**所以 chordify 适合做可视化，不适合做分析内核。**

### 3.4 两条路线对照

| | 路线 A：先切分后识别（chordify 式） | 路线 B：全局最优（Temperley–Sleator 式） |
| --- | --- | --- |
| 边界 | 由 onset 变化决定，独立于和弦 | 与和弦**联合**优化 |
| 过度分割 | 严重（经过音即边界） | 由"和弦切换要付代价"抑制 |
| 装饰音 | 会污染段内音集合 | 作为"不合作的音"被容忍 |
| 实现复杂度 | 低（半天） | 中（2–3 天，需要 DP/Viterbi） |
| 建议 | 先做，作为 fallback 与对比基线 | **最终形态** |

Temperley 的原始表述（[Temperley–Sleator Harmonic Analyzer](https://www.cs.cmu.edu/~sleator/harmonic-analysis/)）是几条偏好规则求全局最优：

> a) 每段内的音要与所选根音兼容；b) 相邻段根音在五度线上接近；c) 和弦变化发生在**强拍**；
> d) 装饰性不协和音要"紧跟一个级进解决的音"。

这四条正好一一对应我们要解决的四个问题，可以直接照抄成代价函数。

### 3.5 推荐管线

```text
[0] 解析 & 归一化
    SMF reader（零依赖，格式细节见 docs/MIDI格式调研.md）
      → tempo map（逐段积分成秒，别乘常数）
      → 量化到节拍网格（PPQ → beat/tick 整数，不做浮点）
      → 轨道角色分类：鼓轨剔除（ch9 / GM 打击乐）、bass 轨、和声轨、旋律轨
      → 输出统一结构：Vec<MidiNote { pitch: 0..127, start_tick, dur_tick, vel, channel }>

[1] 网格剖面
    以 beat 为主单位（可选 1/2、1/3 细分），每格构建：
      chroma      : [u8; 12] 按时长加权（持续音在每格都算）
      bass_pc     : 该格内最低音的 pitch class（没有新音沿用上一个）
      voice_split : 低音区 / 中声区 / 高声区（用于排除旋律）
      metric_w    : 该格的拍位权重（正拍 1.0、半拍 0.6、更弱 0.3）

[2] 候选生成（复用 chord.rs，改成"可枚举 + 带分"）
    每格 → 候选 (root, template, score, missing, extras, root_source)
    剪枝到 N-best（20~30），但保留 DP 需要的转移字段

[3] 全局解码（这一阶段才是"智能"所在）
    状态 = (和弦候选, 当前局部主音)
    转移代价 = -λ1·[和弦不变] ... 见下表
    输出 = 每个网格标签 → 合并成和弦段（harmonic rhythm）

[4] 事件层后处理
    ① 短段吸收：短于半拍的段并入邻段（除非有强证据）
    ② 琶音合并：边界落在"和弦内部音的依次起音"上则撤销
    ③ NCT 标记：被判为外音的短音标成装饰音，附"解决到哪里"
    ④ 离调标注：连续段落落在外调 → "临时主音化 D 小调（2 小节）"

[5] 对接现有层
    把段序列（每段：音集合 + 低音 + 时长）喂给现有 progression.rs / scoring.rs
    → 罗马数字、终止式、走向模板、调外解释全部免费复用
```

**解码代价（建议初值，需要实测调）**

| 项 | 方向 | 说明 |
| --- | --- | --- |
| 和弦持续（自转移） | −0.6 | 一个和弦持续越久越可能是真和弦 |
| 和弦切换 | +0.5 × (1 − metric_w) | 强拍切换便宜、弱拍切换贵（Temperley 规则 c） |
| 候选自身识别分 | −0.1 × rank | 复用 `chord.rs` 的打分 |
| 低音不匹配 | +0.8 | 新和弦的低音应接近当前低音（避免低音乱跳） |
| 五度线距离 | +0.05 × 距离 | 相邻根音在五度线上越近越好（规则 b） |
| 外音数量 | +0.4 × 音数 | 一个和弦解释不了该格多少音 |
| 离调 | +1.2 | 切到"别的调"要付高代价 |

### 3.6 数据模型改造（最关键的一处）

现在全链路是 **pitch class（0..11）+ `u16` 位掩码**。MIDI 必须有八度，否则低音与旋律无法区分。
建议最小侵入：

```rust
// 新增，不影响现有 note.rs
pub struct MidiNote { pub pitch: u8, pub start: u32, pub dur: u32, pub vel: u8, pub channel: u8 }

// 网格剖面
pub struct GridCell { pub chroma: [u8; 12], pub bass: Option<u8>, pub voices: ..., }
```

`chord.rs` 的入参保持 `&[Note]`（音高类），但**再加一个可选的 `bass_hint: Note`**：
现在低音靠"第一个音"的输入约定（README §输入约定），MIDI 模式下低音是算出来的，不能靠约定。
这一处改动小，但决定琶音/转位识别的正确性。

### 3.7 契约建议（旧字段不动）

```rust
#[tauri::command]
fn analyze_midi(bytes: Vec<u8>) -> Result<MidiAnalysis, AnalyzeError>;

pub struct MidiAnalysis {
    pub meta: MidiMeta,              // ppq, bpm(s), timeSignature(s), tracks
    pub notes: usize,
    pub segments: Vec<ChordSegment>, // 核心产物
    pub keys: Vec<KeyCandidate>,     // 复用现有结构
    pub warnings: Vec<String>,
}

pub struct ChordSegment {
    pub start_beat: f64, pub end_beat: f64,
    pub bar: u32, pub beat: f64,     // 定位用
    pub chord: ChordWithLevel,       // 复用现有结构（含 roman/function/explanation）
    pub confidence: u8,
    pub evidence: SegmentEvidence,   // 谁支持这个判定
}

pub struct SegmentEvidence {
    pub chord_tones: Vec<String>,    // 被判为和弦音
    pub nct: Vec<NctNote>,           // 被判为装饰音（含"解决到哪里"）
    pub arpeggiated: bool,           // 是否琶音形态
    pub root_inferred: bool,         // 根音是否未弹（shell voicing）
    pub tonicized: Option<String>,   // 局部临时主音
}
```

`evidence` 是给用户看"为什么这么判"的凭据，也是调试 HMM 的唯一抓手——**别省**。

---

## 4. 关键取舍

| 决策 | 选项 | 建议 | 理由 |
| --- | --- | --- | --- |
| MIDI 解析放哪 | 前端 TS / Rust | **Rust** | 分析内核在 Rust，把整份 MIDI 搬到前端再搬回来做分析不划算；格式笔记已有（`docs/MIDI格式调研.md`）；零依赖可审计 |
| 文件怎么进后端 | Tauri dialog+fs 插件 / 前端读 ArrayBuffer 再 invoke | **前端读 → invoke(bytes)** | 不加插件、不改 capability 配置；Tauri 2 传 `Vec<u8>` 开销可接受（一个 MIDI 通常 < 200 KB） |
| 网格粒度 | 固定小节 / 固定拍 / 可细分 | **beat 为基本单位**，边界候选放宽到 1/2、1/3 | 一小节两个和弦是 2 拍粒度；三连音在爵士里普遍 |
| 状态空间 | 全枚举 / N-best 剪枝 | **N-best 剪枝（20~30）** | 55 模板 × 12 根音 × 每格，全枚举做 Viterbi 也能跑，但剪枝后好调 |
| 调性 | 全局单吊 / 滑窗 | **全局 + 滑窗双输出** | 全局回答"这是什么调"，滑窗回答"哪里离调了" |
| 是否引入 ML | — | **不引入** | 当前需求用规则 + DP 就能覆盖；引入模型会失去"可解释"这个核心卖点（README 的定位就是"解释得通"） |

---

## 5. 怎么知道做对了

### 5.1 指标（照抄 MIR 社区，避免自说自话）

用 [mir_eval.chord](https://mir-eval.readthedocs.io/latest/api/chord.html) 的口径：

- **root**：只比根音（最能反映"是否听错和弦"）
- **majmin** / **triads** / **sevenths** / **tetrads**：逐级加严
- **mirex**：≥3 个音级重合就算对
- **seg / overseg / underseg**：**专测"一小节两个和弦"这种分段问题**

### 5.2 语料：自己造，别等公开数据集

MIDI 的真值（标注）比音频容易造得多。建议写一个生成器（复用 `scripts/smf-writer-demo.mjs` 的写法）：

```text
基础进行：ii–V–I、I–vi–ii–V、卡农、布鲁斯 12 小节、节奏 changes 片段
每个进行生成 6 个变体：
  v0 柱式（干净真值）
  v1 琶音（音依次 +40ms，1 拍内）
  v2 装饰音（正拍前加一个 1/32 的邻音）
  v3 无根音（左手只弹 3-7）
  v4 一小节两个和弦（第 3 拍换和弦）
  v5 离调（第 3 小节插 V7/x → x）
```

每个变体都带**逐段真值标签**（Harte 语法），就能自动算上面 6 个指标。
这套语料同时是最好的回归测试：以后改打分参数，跑一遍就知道有没有退化。

### 5.3 把本次探针固化成基线

`scripts/chord-baseline-probe.rs` 是本次调研用的探针（`rustc` 直接编译，**不经过 cargo**，
见文件头注释），它把 §1.2 的实测表固化成可重跑的基线。改内核后重跑，一眼看出哪些用例翻车/转正。

---

## 6. 分期建议

| 阶段 | 内容 | 产出 | 量级 |
| --- | --- | --- | --- |
| **P0** | 模板扩容（L1）+ 根音解耦（L2）+ 探针全部转正 | `chord.rs` 单测；探针基线从 5 个 ❌ 变 ✅ | 1 天 |
| **P1** | `midi.rs`（SMF 解析）+ 固定网格切片 + 前端拖文件 | 能对 MIDI 输出"每拍一个和弦"，会过度分割 | 1–2 天 |
| **P2** | `rhythm.rs`：网格剖面 + 候选 + Viterbi 解码 | 真正的 MIDI 模式（能处理一小节多个和弦、无根音） | 2–3 天 |
| **P3** | 后处理：琶音合并、NCT 标记、离调轨迹、`evidence` 可视化 | "分析报告"级输出 | 2 天 |

**先 P0 再 P1**：不走 P0 直接上 MIDI，等于在错误的音集合上做分段，调参永远调不出来。

---

## 7. 参考资料

- [Temperley–Sleator Harmonic Analyzer](https://www.cs.cmu.edu/~sleator/harmonic-analysis/) — 偏好规则 + 全局最优，本方案的直接蓝本（页面标注 obsolete，最新在 [Melisma](http://www.link.cs.cmu.edu/melisma/)）
- [music21 User's Guide · Chapter 9 Chordify](https://music21.org/music21docs/usersGuide/usersGuide_09_chordify.html) — salami-slicing 的权威实现，以及"过度分割"的官方自证（巴赫众赞歌里 4 个假属七）
- [mir_eval.chord](https://mir-eval.readthedocs.io/latest/api/chord.html) — 和弦评估指标与 Harte 标签语法的工程化定义
- Harte, *Towards Automatic Extraction of Harmony Information from Music Signals*, PhD thesis, QMUL 2010 — Harte 标签语法的出处
- [Humdrum `tsroot`](https://extras.humdrum.org/man/tsroot/) — 符号（Humdrum/kern）层面的和弦根音判定工具，可作为"先指定调、再判和弦"这种顺序的对照
- 本仓库 [`docs/MIDI格式调研.md`](./MIDI格式调研.md) — SMF 字节级格式与读写坑清单（写 `midi.rs` 前必读）
