/**
 * 草稿领域模型（分镜 / 资产 / 媒体变体 / 剧集清单）给 core 用的一份。
 *
 * 类型与五个读取函数和 src/domain/types.ts 逐字相同：core 只能 import core 里的文件，
 * 渲染层原件改成从这里 re-export 之前，两边由 scripts/test-core-mirrors.ts 核对不许漂移。
 * 类型本身漂移时，渲染层把 domain 的 Shot / Asset 传给 core 函数就会编译失败，编译器兜着。
 */
import { mediaVariantUrl } from '../media/mediaUrl';

export type DirectorStylePluginState = {
  enabled: boolean;
  /**
   * 伪状态，只为兼容旧数据留着，没有任何读者认它（提示词注入与页面都读事实，见 `directorStyleActive`）。
   * 提取中 / 提取失败不存（6.1）：从 `field:directorStylePlugin` 槽的任务活动派生。「已生效」= enabled && summary。
   */
  status: 'active';
  directorId?: string;
  filmStyleId?: string;
  directorName?: string;
  filmTitle?: string;
  knowledgeVersion?: number;
  confidence?: number;
  intensity?: number;
  summary?: string;
  reasons?: string[];
  imageGuidance?: string;
  videoGuidance?: string;
  validationRules?: string[];
  negativeRules?: string[];
  extractedAt?: string;
};

/** 导演风格有没有提取结果（事实：summary 存在）；「已生效」再加 plugin.enabled（见 directorStyleActive）。 */
export function directorStyleExtracted(plugin: Pick<DirectorStylePluginState, 'summary'> | null | undefined) {
  return Boolean(plugin && String(plugin.summary || '').trim());
}

/** 导演风格「已生效」= 开着且有提取结果：提示词注入（core）与页面共用这一份判定，不读 status 伪状态。 */
export function directorStyleActive(plugin: Pick<DirectorStylePluginState, 'enabled' | 'summary'> | null | undefined) {
  return Boolean(plugin?.enabled) && directorStyleExtracted(plugin);
}

export type EpisodeKind = 'story' | 'promo';

export type PromoEpisodeConfig = {
  /** 参与合并提取的正片集 ID，按集序排列。 */
  sourceEpisodeIds: string[];
  promptTemplateKey?: string;
  promptTemplateVariantId?: string;
  promptTemplateName?: string;
  /** 最近一次宣发剧本提取成功的时间与任务。 */
  extractedAt?: number;
  extractionTaskId?: string;
};

export type EpisodeAssetManifest = {
  /** AI 推理确认的本集完整资产清单。 */
  aiAssetIds?: string[];
  /** 用户手动加入本集的资产。 */
  manualIncludeAssetIds?: string[];
  /** 用户手动从本集移除的资产。 */
  manualExcludeAssetIds?: string[];
  /** 宣发集只认领不新建：AI 在现有资产里找不到对应项时列在这里，供人工绑定或手动建卡。 */
  unmatched?: Array<{ name: string; type?: string; evidence?: string }>;
  updatedAt?: string;
};

export type EpisodeSpeakerAssetIssueKind = 'not-in-manifest';

export type EpisodeSpeakerAssetIssueRecord = {
  /** 规范化后的角色名，同一集内唯一。 */
  key: string;
  /** 显示用的角色名（资产的 canonicalName，没有资产时用剧本里的写法）。 */
  name: string;
  issue: EpisodeSpeakerAssetIssueKind;
  detectedAt: string;
  /** 人工点“已处理”的时间；有值即不再显示。 */
  resolvedAt?: string;
};

export function isPromoEpisode(episode: { kind?: string } | null | undefined): boolean {
  return String(episode?.kind || '') === 'promo';
}

export function storyEpisodesOf<T extends { kind?: string }>(episodes: T[] | null | undefined): T[] {
  return (episodes || []).filter((episode) => !isPromoEpisode(episode));
}

export type Shot = {
  id: string;
  revision?: number;
  /** 分镜切分阶段最终写入该镜头的原始结构化数据，不包含后续图片/视频生产字段。 */
  storyboardSplitData?: Record<string, unknown>;
  index?: number;
  episodeId?: string;
  episodeTitle?: string;
  originalText?: string;
  description?: string;
  visualDescription?: string;
  narration?: string;
  duration?: number;
  dialogueCharCount?: number;
  aspectRatio?: string;
  imageUrl?: string;
  videoUrl?: string;
  status?: string;
  /** null 表示人工取消标记；缺省才兼容旧视频候选审核。 */
  approved?: boolean | null;
  /** 视频审看的第三个选项：能用但要手动修剪。导出剪映时列进《需要修剪.md》并在导出弹窗里提示。 */
  trimRequired?: boolean;
  videoPromptValidation?: unknown;
  visualThesis?: string;
  visualIntensity?: number;
  visualPayoff?: string;
  directorPlan?: ShotDirectorPlan;
  cutPlan?: ShotCutPlan;
  storyboardFrameCount?: number;
  /** All current and archived storyboard boards; media belongs to each board. */
  storyboardBoards?: ShotStoryboardBoardImage[];
  /**
   * 通用贪心层（菜单「贪心15s/30s」）装出来的容器分镜才有：容器上限与来源基础分镜编号。
   * 有它的分镜不看流程，故事板/视频提示词一律走通用多故事板模板；读的时候用 shotGreedyContainer()。
   */
  greedyContainer?: ShotGreedyContainer;
  frameCountReason?: string;
  negativePrompt?: string;
  pacingWarning?: string;
  assetReferenceBindings?: ShotAssetReferenceBinding[];
  requiredAssetIds?: string[];
  /** 分镜切分阶段确定的逐镜资产；当前由 scenes 作为权威场景来源。 */
  requiredAssets?: ShotRequiredAssets;
  continuityStartState?: ContinuityState;
  continuityEndState?: ContinuityState;
  voiceover?: ShotVoiceover;
};

export type ShotRequiredAssets = {
  characters?: string[];
  scenes?: string[];
  items?: string[];
  symbols?: string[];
  voices?: string[];
  videos?: string[];
};

export type ShotDirectorMicroBeat = {
  purpose?: string;
  speaker?: string;
  text?: string;
  offscreen?: boolean;
  stimulus?: string;
  action?: string;
  reaction?: string;
  result?: string;
  cameraPurpose?: string;
};

export type ShotDirectorPlan = {
  sceneObjective?: string;
  conflict?: string;
  result?: string;
  spaceMap?: string;
  businessTrack?: string;
  intensity?: number;
  startState?: Record<string, unknown> | string;
  endState?: Record<string, unknown> | string;
  microBeats?: ShotDirectorMicroBeat[];
  [key: string]: unknown;
};

export type ShotVisualBeat = {
  beatId: number;
  role: string;
  content: string;
  storyboardAnchor: boolean;
  /** 30s贪心版：anchor 归属的故事板（1 或 2），非 anchor 为 null。 */
  storyboardBoardId?: number | null;
  /** 30s贪心版：anchor 在该张故事板内的阅读顺序，从 1 开始；非 anchor 为 null。 */
  storyboardFrameIndex?: number | null;
  [key: string]: unknown;
};

/** 30s贪心版：分镜切分阶段直接规划好的故事板分组，一条分镜 1—2 张，每张最多 9 格。 */
export type ShotStoryboardBoard = {
  boardId: number;
  frameCount: number;
  anchorBeatIds: number[];
  coverageStartBeatId?: number;
  coverageEndBeatId?: number;
  purpose?: string;
  [key: string]: unknown;
};

/** 30s贪心版：联合阶段按 storyboardPlan 逐张生成的故事板提示词与成图。 */
export type ShotStoryboardBoardImage = {
  boardId: number;
  archived?: boolean;
  frameCount?: number;
  anchorBeatIds?: number[];
  coverageStartBeatId?: number;
  coverageEndBeatId?: number;
  purpose?: string;
  imagePrompt?: string;
  imageUrl?: string;
  imageBase64?: string;
  imageVariants?: MediaVariant[];
  selectedImageId?: string;
  mediaRef?: MediaRef;
};

/** Active storyboard boards only. Archived paid candidates never become generation targets. */
export function activeShotBoards(shot: Record<string, unknown> | null | undefined): ShotStoryboardBoardImage[] {
  const boards = Array.isArray(shot?.storyboardBoards) ? shot.storyboardBoards as ShotStoryboardBoardImage[] : [];
  return boards.filter((board) => board && typeof board === 'object').map((board, index) => ({ ...board, boardId: Number(board.boardId) > 0 ? Number(board.boardId) : index + 1 })).filter((board) => board.archived !== true);
}

export function shotStoryboardBoards(shot: Record<string, unknown> | null | undefined): ShotStoryboardBoardImage[] {
  return activeShotBoards(shot).sort((left, right) => left.boardId - right.boardId).map(withBoardImageUrl);
}

/**
 * 读出来的故事板必须带「当前图地址」：规范存放处只有候选列表 + 选中 id（写时收敛不再写 imageUrl），
 * 但视频提交取额外故事板图、界面主图都按 board.imageUrl 读——这里按选中候选（没选中取第一条）补出来，
 * 别名形态（有 imageUrl）与规范形态（只有候选）读出同一个值。
 */
function withBoardImageUrl(board: ShotStoryboardBoardImage): ShotStoryboardBoardImage {
  const selected = board.imageVariants?.find((variant) => variant?.id === board.selectedImageId) || board.imageVariants?.[0];
  const url = mediaVariantUrl(selected);
  return url ? { ...board, imageUrl: url } : board;
}

/** 贪心容器分镜的标记：由主进程 05.5 装配写入；人工合并同步重编号来源与节拍段。 */
export type ShotGreedyContainer = {
  /** v3 保留所选提示词模板；旧版继续使用历史通用多板模板。 */
  packingVersion?: number;
  /** 容器时长上限（秒），15 或 30。 */
  targetDuration: number;
  /** 装进来的基础分镜在那一轮切分里的编号，从 1 起。 */
  sourceShotIds: number[];
  /** 单条基础分镜自身就超过上限，被当成异常容器原样放出，等人处理。 */
  overflow?: boolean;
  issues?: Array<Record<string, unknown>>;
};

/**
 * 读贪心容器标记。老数据只在 cutPlan 上留了 greedyContainer=true 的话也认，上限按时长推：
 * 超过 15 秒的只能是 30 秒容器。不是容器返回 null。
 */
export function shotGreedyContainer(shot: Record<string, unknown> | null | undefined): ShotGreedyContainer | null {
  const record = (shot && typeof shot === 'object' ? shot : {}) as Record<string, unknown>;
  const top = (record.greedyContainer && typeof record.greedyContainer === 'object' ? record.greedyContainer : {}) as Record<string, unknown>;
  const numbers = (value: unknown) => (Array.isArray(value) ? value.map(Number).filter((item) => Number.isFinite(item)) : []);
  if (Number(top.targetDuration) > 0) {
    return {
      targetDuration: Number(top.targetDuration),
      ...(Number(top.packingVersion) > 0 ? { packingVersion: Number(top.packingVersion) } : {}),
      sourceShotIds: numbers(top.sourceShotIds),
      ...(top.overflow === true ? { overflow: true } : {}),
      ...(Array.isArray(top.issues) && top.issues.length > 0 ? { issues: top.issues as Array<Record<string, unknown>> } : {})
    };
  }
  const plan = (record.cutPlan && typeof record.cutPlan === 'object' ? record.cutPlan : {}) as Record<string, unknown>;
  if (plan.greedyContainer === true) {
    return { targetDuration: Number(record.duration) > 15 ? 30 : 15, sourceShotIds: numbers(plan.sourceShotIds) };
  }
  return null;
}

/** 这条分镜是不是真的有第 2 张故事板（只有 30s贪心版的 storyboardBoardCount=2 才会有）。 */
export function shotHasExtraStoryboards(shot: Record<string, unknown> | null | undefined): boolean {
  const record = (shot && typeof shot === 'object' ? shot : {}) as Record<string, unknown>;
  return activeShotBoards(record).length > 1;
}

// 成片对齐版的切分计划：分镜阶段产出，联合阶段据此决定格数、环境常量与首尾帧。
export type ShotCutPlan = {
  sceneGoal?: string;
  closeReason?: string;
  result?: string;
  businessAction?: string;
  spaceMap?: string;
  continuityLock?: string;
  startState?: string;
  endState?: string;
  frameCount?: number;
  secondsPerFrame?: number;
  visualBeatCount?: number;
  videoMicroShotCount?: number;
  storyboardFrameCount?: number;
  secondsPerVisualBeat?: number;
  visualBeatType?: 'fast_conflict' | 'normal_dialogue' | 'reaction_reveal' | 'action_montage' | 'process_hold' | 'contrast_progression' | string;
  visualBeatPlan?: string[] | ShotVisualBeat[];
  needsVisualBeatReplan?: boolean;
  /** 30s贪心版：本条需要几张故事板，只能是 1 或 2；等于 storyboardPlan.length。 */
  storyboardBoardCount?: number;
  /** 30s贪心版：所有故事板的关键锚点格数之和。 */
  storyboardTotalFrameCount?: number;
  /** 30s贪心版：每张故事板覆盖哪些 anchor beat，由分镜切分阶段一次性决定。 */
  storyboardPlan?: ShotStoryboardBoard[];
  keyReactionWindows?: string;
  frameCountReason?: string;
  visualBeatCountReason?: string;
  storyboardPlanReason?: string;
  durationBreakdown?: string;
  [key: string]: unknown;
};

export type CharacterContinuityState = {
  name: string;
  position?: string;
  pose?: string;
  facing?: string;
  gaze?: string;
  outfitVariant?: string;
  injury?: string;
  emotion?: string;
  heldProps?: string[];
};

export type ContinuityState = {
  schemaVersion?: 'continuity-v1' | string;
  scene?: string;
  sceneState?: string;
  lightingState?: string;
  characters?: CharacterContinuityState[];
  props?: Record<string, unknown>;
  ongoingAction?: string;
  directingState?: {
    narrativeFocus?: string;
    emotionalPressure?: string;
    visualMotif?: string;
    pendingReveal?: string;
  };
  [key: string]: unknown;
};

export type ShotVoiceover = {
  audio?: MediaRef;
  audioVariants?: MediaVariant[];
  selectedAudioId?: string;
  duration?: number;
  words?: Array<{ text: string; start: number; end: number }>;
  speaker?: string;
  charCount?: number;
  status?: 'idle' | 'queued' | 'processing' | 'success' | 'error' | string;
  statusMessage?: string;
  updatedAt?: string;
};

export type MediaVariant = {
  id: string;
  /** Historical source candidate retained after a manual merge; explicit selection may reuse it. */
  archived?: boolean;
  url?: string;
  originalUrl?: string;
  mediaRef?: MediaRef;
  source?: 'generated' | 'uploaded' | 'imported' | string;
  prompt?: string;
  model?: string;
  createdAt?: string;
  providerTaskId?: string;
  billingRecordId?: string | number;
  duration?: number;
  words?: Array<{ text: string; start: number; end: number }>;
  speaker?: string;
  charCount?: number;
  label?: string;
  stateName?: string;
  parentVariantId?: string;
  approvedAt?: string;
  version?: number;
  aspectRatio?: string;
  batchId?: string;
  approvalStatus?: 'draft' | 'approved' | 'rejected' | string;
  assetReferenceBindings?: ShotAssetReferenceBinding[];
  /**
   * 新生成、人还没看过：宿主写回分镜图 / 视频 / 配音结果时打上，预览里切到这一条或导出剪映后渲染层删掉。
   * 界面显示 NEW（docs/shot-task-status-and-new-badge-design.md 3.2）。
   */
  unseen?: boolean;
};

export type ShotAssetReferenceBinding = {
  kind: 'character' | 'scene' | 'item' | 'symbol' | string;
  assetId: string;
  name: string;
  canonicalName?: string;
  variantName?: string;
  /** 资产的其它叫法（剧本里的真名、外号）。提示词写 {角色:别名} 时靠它对到这张参考图，只按整名匹配。 */
  aliases?: string[];
  imageVariantId?: string;
  imageUrl?: string;
  approved?: boolean;
  referenceRole?: 'identity' | 'look' | 'space' | 'prop' | 'symbol' | string;
  mayControl?: string[];
  mustNotControl?: string[];
};

export type Asset = {
  id: string;
  manifestEntryId?: string;
  name?: string;
  type?: 'character' | 'scene' | 'item' | 'symbol' | 'voice' | 'video' | 'sound' | string;
  scope?: 'common' | 'episode' | string;
  episodeId?: string;
  episodeTitle?: string;
  canonicalName?: string;
  /** 首次出场人物介绍使用的公开身份短句；旧资产允许为空。 */
  introIdentity?: string;
  aliases?: string[];
  variantName?: string;
  parentAssetId?: string;
  identityAnchors?: string[];
  variantState?: {
    name?: string;
    cause?: string;
    validFrom?: string;
    validUntil?: string;
    stableChanges?: string[];
    transientChanges?: string[];
  };
  sourceEpisodeIds?: string[];
  firstSeenEpisodeId?: string;
  firstSeenEpisodeNumber?: number;
  firstSeenEpisodeTitle?: string;
  changeEvidence?: string;
  appearanceNote?: string;
  continuityKey?: string;
  visualKind?: 'symbol' | 'manifestation' | string;
  characterForm?: 'human' | 'sprite' | string;
  continuityPriority?: number;
  libraryScope?: 'global' | 'personal' | string;
  prompt?: string;
  description?: string;
  novelType?: string;
  category?: string;
  gender?: string;
  age?: string;
  imageUrl?: string;
  imageBase64?: string;
  imageVariants?: MediaVariant[];
  selectedImageId?: string;
  approvedImageId?: string;
  approvedReferenceAt?: string;
  referenceImage?: string;
  referenceImages?: string[];
  referenceImageMediaRefs?: MediaRef[];
  referenceVideos?: string[];
  referenceVideoMediaRefs?: MediaRef[];
  referenceAudios?: string[];
  referenceAudioMediaRefs?: MediaRef[];
  mediaRef?: MediaRef;
  characterViews?: Record<string, string | undefined>;
  states?: Array<Record<string, unknown> & { imageVariants?: MediaVariant[]; selectedImageId?: string; mediaRef?: MediaRef; imageBase64?: string }>;
  sceneStates?: Array<Record<string, unknown> & { imageVariants?: MediaVariant[]; selectedImageId?: string; mediaRef?: MediaRef; imageBase64?: string }>;
  spatialLayout?: {
    scale?: string;
    zones?: string[];
    entrances?: string[];
    passages?: string[];
    fixedAnchors?: string[];
    orientation?: string[];
    lightingState?: string;
  };
  audios?: Array<{ id?: string; name?: string; url?: string; mediaRef?: MediaRef }>;
  voiceId?: string;
  ttsVoice?: string;
  voiceResourceId?: string;
  voiceDesignSpeakerId?: string;
  voiceDesignPrompt?: string;
  voiceDesignDemoAudio?: MediaRef;
  voiceDesignRemainingTimes?: number;
  // 音色样本：走视频渠道生成一段固定台词的短视频后抽出的音轨，存于 audios[0]，视频提交时作为参考音频
  voiceSampleText?: string;
  voiceSampleVideo?: MediaRef;
  attachmentUrl?: string;
  fileName?: string;
  username?: string;
  /** 素材校验的结果事实（success / failed / none）；进行中与上次失败从任务活动派生（6.1）。 */
  validationStatus?: 'success' | 'failed' | 'none' | string;
  validationAssetId?: string;
  validationMessage?: string;
  url?: string;
  status?: string;
};

export type MediaRef = {
  url?: string;
  localUrl?: string;
  localId?: string;
  originalName?: string;
  mimeType?: string;
  storageProvider?: string;
  [key: string]: unknown;
};
