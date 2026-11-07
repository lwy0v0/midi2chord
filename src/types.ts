/**
 * 与 Rust 后端的 JSON 契约（一一对应）。
 *
 * 命名规则：后端用 `#[serde(rename_all = "camelCase")]`，因此这里全部是 camelCase。
 * 旧字段（`keys[].key`、`keys[].chords[].chord/name/level`）保持兼容，其余为新增字段。
 */

/** 输入校验错误，带行列定位（0 起）。 */
export interface AnalyzeError {
  /** emptyInput / emptyNote / invalidNote / tooFewNotes / notTauri / unknown */
  code: string;
  message: string;
  row: number | null;
  col: number | null;
}

/** 单个和弦在某个调下的分析结果。 */
export interface ChordWithLevel {
  // ---------- 旧字段 ----------
  /** 和弦音名（升号拼写）。 */
  chord: string[];
  /** 和弦名（升号拼写）。 */
  name: string;
  /** 音级 1..7；0 表示根音不在该调音阶内。 */
  level: number;
  // ---------- 新增字段 ----------
  /** 按调号拼写的和弦记号，转位带斜杠低音，如 `C/E`、`C7(no5)`。 */
  symbol: string;
  root: string;
  bass: string;
  quality: string;
  qualityLabel: string;
  suffix: string;
  kind: string;
  /** 中文结构名，如「属七和弦（省略 5）」。 */
  label: string;
  roman: string;
  degree: number;
  /** T / S / D / — */
  function: string;
  functionLabel: string;
  inScale: boolean;
  /** diatonic / harmonicMinor / secondaryDominant / modalMixture / neapolitan / chromatic */
  category: string;
  explanation: string;
  tags: string[];
  /** 其它可能的解释。 */
  alternatives: string[];
  /** 按调号拼写的音名。 */
  notes: string[];
  missing: string[];
  extras: string[];
  warning: string | null;
}

export interface CadenceInfo {
  at: number;
  kind: string;
  formula: string;
  description: string;
}

export interface TemplateInfo {
  name: string;
  start: number;
  end: number;
  formula: string;
  note: string;
}

/** 一个候选调。 */
export interface KeyCandidate {
  // ---------- 旧字段 ----------
  key: string;
  chords: ChordWithLevel[];
  // ---------- 新增字段 ----------
  isMajor: boolean;
  mode: string;
  displayName: string;
  fullName: string;
  /** 综合排序分（含主和弦位置、终止式、走向模板加成），通常 0..1.5。 */
  score: number;
  /** 0..100 的整数百分比：调内贴合度（加权平均，不含位置与终止式加成）。 */
  confidence: number;
  rank: number;
  matched: number;
  explained: number;
  total: number;
  romans: string[];
  cadences: CadenceInfo[];
  templates: TemplateInfo[];
  reasons: string[];
  scaleNotes: string[];
  accidentals: number;
}

/** 分析结果。 */
export interface AnalysisResult {
  keys: KeyCandidate[];
  /** 与调性无关的和弦识别结果。 */
  recognized: string[];
  warnings: string[];
  total: number;
}

/** 格子级错误提示。 */
export interface CellError {
  row: number;
  col: number;
  message: string;
}

// =====================================================================
// MIDI 分析（Loop 调性与和弦提取）
// 与 `src-tauri/src/extract.rs` 的结构一一对应；后端用 camelCase 序列化。
// =====================================================================

/**
 * 一个音符在和声里的角色。
 *
 * 取值直接对应界面颜色：`root/third/fifth/seventh` 是和弦骨架，
 * `extension` 是骨架外的和弦音，`melody` 是判定的非和弦音，`outside` 落在所有和弦段之外。
 */
export type NoteRole =
  | "root"
  | "third"
  | "fifth"
  | "seventh"
  | "extension"
  | "melody"
  | "outside";

/** 折叠后的基础和弦。 */
export interface BasicChord {
  /** 根音音高类 0..11（0 = C）。 */
  root: number;
  /** 三和弦后缀："" / "m" / "dim" / "aug" / "sus4" / "sus2"。 */
  triadSuffix: string;
  seventh: boolean;
  /** 七音相对根音的半音数（10 = 小七，11 = 大七）。 */
  seventhInterval: number;
  /** 骨架之外的音（相对根音半音数）。 */
  tensions: number[];
  /** 低音音高类 0..11。 */
  bass: number;
  /** 低音是否不属于该和弦（真正的斜杠和弦）。 */
  slash: boolean;
  /** 完整记号，如 `Cmaj7`、`Am7`、`C/E`。 */
  symbol: string;
  /** 0..100。 */
  confidence: number;
}

/** 代表音：同一和弦音只取一个实例（时值最长、其次音高最低）。 */
export interface VoicingNote {
  /** MIDI 音高（带八度）。 */
  pitch: number;
  /** 音名，如 `C4`。 */
  name: string;
  role: NoteRole;
  startBeat: number;
  durBeats: number;
  /** 该音高类在这一段里一共出现了几次（八度加倍、叠层会大于 1）。 */
  count: number;
}

/** 一个和弦段。 */
export interface ChordSegment {
  startBeat: number;
  lenBeats: number;
  /** 1 起的小节号（可为小数）。 */
  bar: number;
  /** 小节内第几拍（1 起）。 */
  beatInBar: number;
  chord: BasicChord;
  /** 该段低音是否为前一个和弦的残留（持续音）。 */
  pedalBass: boolean;
  /** 被判为和弦音的音名。 */
  chordTones: string[];
  /** 落在段内但没被判为和弦音的音名（旋律/装饰）。 */
  excluded: string[];
  /** 代表音（画卷帘窗用，每段只有 3–4 个）。 */
  voicing: VoicingNote[];
}

/** 音符级标注：每个音符一条，用于钢琴卷帘窗上色。 */
export interface AnnotatedNote {
  pitch: number;
  name: string;
  /** 来源轨道下标。 */
  track: number;
  startBeat: number;
  durBeats: number;
  velocity: number;
  /** 归属的和弦段下标；null 表示不在任何段内。 */
  segment: number | null;
  role: NoteRole;
  /** 判定权重 0..1（时值 × 是否成簇）。 */
  weight: number;
  /** 同一起点附近是否有别的音。 */
  clustered: boolean;
}

/**
 * 一个候选调，连同它下面的解释。
 *
 * 调性判断本来就有歧义（关系大小调、五度关系都能自圆其说），
 * 所以后端不只给一个答案，而是把可能性摊开、带排序依据，让用户自己挑。
 */
export interface KeyOption {
  name: string;
  /** 音级分布分 0..100（相对最高分）。 */
  pitchScore: number;
  /** 和弦解码总代价，越小越好。 */
  chordCost: number;
  /** 综合排序分，越小越靠前。 */
  rankScore: number;
  /** 开头第一个和弦就是主和弦——很能说明问题，但不该单独决定结论。 */
  startsOnTonic: boolean;
  endsOnTonic: boolean;
  /** 主和弦占总时长 %。 */
  tonicShare: number;
  /** 低音落在主音 %。 */
  bassOnTonic: number;
  /** 是否是后端默认选中的那个。 */
  selected: boolean;
  /** 这个调下的和弦进行。 */
  progression: string[];
  /** 这个调下每段的罗马数字（与 segments 一一对应）。 */
  romans: string[];
}

/** 一次 MIDI 分析的完整结果。 */
export interface MidiAnalysisResult {
  bpm: number;
  /** false 表示文件里没有速度事件，bpm 是默认值。 */
  bpmExplicit: boolean;
  beatsPerBar: number;
  bars: number;
  /** 第一个音距离起点的拍数（> 0.5 视为弱起）。 */
  pickupBeats: number;
  /** 主判定调性，如 `C 大调`。 */
  keyName: string;
  keyConfidence: number;
  /** 候选调性 [名称, 分数]，按可信度排序。 */
  keyCandidates: [string, number][];
  /** 仅由音级分布得出的调性（与和弦序列互校）。 */
  keyFromPitches: string;
  /** 置信度偏低时的原因说明。由后端给出（它知道为什么低），界面直接显示，不要自己写死文案。 */
  keyNote: string;
  /** 折叠后的和弦进行（连续相同已合并）。 */
  progression: string[];
  /** 每段对应的罗马数字。 */
  romans: string[];
  segments: ChordSegment[];
  /** 声部识别结果的人话描述。 */
  layers: string[];
  warnings: string[];
  /** 只有单声部（没有和声层），此时不给和弦。 */
  singleLine: boolean;
  /** 全部音符的逐条标注。 */
  notes: AnnotatedNote[];
  /** 所有候选调及其解释，按综合分排序（第一个是默认选中的）。 */
  keyOptions: KeyOption[];
}
