/**
 * 草稿编辑的集内视图类型：剧集、剧本场景、编辑中的分镜。从 DraftEditorPage 搬来，
 * 渲染层、宿主调度器和载荷构造都按这一份读草稿（docs/architecture-design.md 3.6）。
 */
import type { ContinuityState, EpisodeAssetManifest, EpisodeSpeakerAssetIssueRecord, MediaVariant, PromoEpisodeConfig, Shot } from './model';
import type { VideoContentRejection } from '../batch/pipeline';

// 分镜上限只剩最大15s / 最大30s（固定秒数的 10s / 15s 两项 2026-09-24 删掉）；贪心15s / 贪心30s 复用同一个数当容器上限。
export type StoryboardTargetDuration = 15 | 30;

export type ExtractableAssetType = 'character' | 'scene' | 'item' | 'symbol' | 'voice';

export type VisualAssetType = Exclude<ExtractableAssetType, 'voice'>;

export type AssetScopeFilter = 'episode' | 'common' | 'all';

export type Episode = {
  id: string;
  title: string;
  text: string;
  kind?: 'story' | 'promo';
  promo?: PromoEpisodeConfig;
  assetExtractedAt?: number;
  assetExtractionTaskId?: string;
  sourceText?: string;
  scriptValidation?: unknown;
  scriptScore?: unknown;
  assetManifest?: EpisodeAssetManifest;
  speakerAssetIssues?: EpisodeSpeakerAssetIssueRecord[];
};

export type ScriptBlock = {
  type?: string;
  text?: string;
  character?: string;
};

export type ScriptScene = {
  id: string;
  chapter?: string;
  title?: string;
  sceneHeader?: string;
  summary?: string;
  characters?: string[];
  content?: string | ScriptBlock[];
  batchIndex?: number;
};

export type EditableShot = Shot & {
  episodeId?: string;
  episodeTitle?: string;
  scriptContent?: string;
  content?: string;
  narration?: string;
  requiredAssetsText?: string;
  imageBase64?: string;
  imageVariants?: MediaVariant[];
  selectedImageId?: string;
  referenceFrameBase64?: string;
  referenceImages?: string[];
  shotImages?: string[];
  imagePrompt?: string;
  videoPrompt?: string;
  videoDuration?: string;
  estimatedDuration?: number;
  sourceEstimatedDuration?: number;
  dialogueCharCount?: number;
  voiceCharCount?: number;
  comicPanelCount?: number;
  videos?: MediaVariant[];
  selectedVideoId?: string;
  videoPromptLengthErrorSource?: string;
  /**
   * Miora 正文超限后自动精简视频提示词的记录。sourcePrompt 是超限的那一版：
    * 在途由真实任务判定；count 只记录历史，精简后最终载荷仍超限则继续精简。
   * P2 起「谁接着交视频」不再靠这个字段（旧的 autoVideo 子字段已删）：渲染层单条超限改走
   * `workflows.start({ kind: 'shotRegenerate', steps: ['video'] })`，批量小助手这一步自己按数据重排。
   * 见 core/payloads/shotVideo.ts 的 MioraCondenseContinuation。
   */
  videoPromptLengthRedo?: { count: number; excess: number; sourcePrompt: string; at: number };
  /**
   * 视频因内容违规失败（主进程写）：批量小助手据此重推提示词 → 重出故事板 → 再生视频。
   * at 之后成功的重推任务会记下完成时刻 / 故事板张号；出片后整笔清掉。count 是连续违规次数。
   */
  videoContentRejection?: VideoContentRejection;
  /**
   * 配音导演 → 逐画格语音合成的写入计划（宿主写，流程 `comicVoiceDirection`）：`at` 写入时刻、`directTaskId` 导演任务号，
   * `taskTemplate` 是 `tts:<shotId>` 步入队用的现成任务载荷（不带密钥），TTS 成功后由宿主摘掉。
   */
  voiceDirectionPlan?: { at?: number; directTaskId?: string; taskTemplate?: Record<string, unknown>; [key: string]: unknown };
  approved?: boolean | null;
  remark?: string;
  promptInstruction?: string;
  timeline?: Array<{ videoId?: string; startTime?: number | null; endTime?: number | null }>;
  videoPromptValidation?: unknown;
  continuityStartState?: ContinuityState;
  continuityEndState?: ContinuityState;
  extraInfo?: Record<string, unknown>;
};

export type ShotRequiredAssets = NonNullable<EditableShot['requiredAssets']>;

export type PromptAssetKind = 'character' | 'scene' | 'item' | 'symbol' | 'voice';
