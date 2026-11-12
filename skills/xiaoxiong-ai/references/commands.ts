import type { Draft } from './draft';
import type { Episode, ScriptScene } from './editor';
import type { Asset, MediaVariant, Shot, ShotStoryboardBoardImage } from './model';
import type { DataEntity } from './fields';
import type { ImageEditorCommand } from '../imageEditor/commands';

export type CommandOrigin = { kind: 'user'; windowId: string } | { kind: 'task'; taskId: string } | { kind: 'workflow'; workflowId: string; stepKey: string } | { kind: 'migration'; name: string };
export type MediaSubject =
  | { entity: 'shot'; shotId: string; slot: 'image'; boardId: number }
  | { entity: 'shot'; shotId: string; slot: 'video' | 'audio' }
  | { entity: 'asset'; assetId: string; slot: 'image' }
  | { entity: 'draft'; field: 'videoCover' | 'comicCover'; aspectRatio: string }
  | { entity: 'draft'; field: 'globalRefImages' }
  | { entity: 'canvasNode'; nodeId: string; slot: 'image' | 'video' };
export type ShotReplaceScope = { kind: 'episode' } | { kind: 'shots'; shotIds: string[]; ownerTaskIds: string[] } | { kind: 'segment'; groupId: string; index: number };
/** 基准版本：用户命令带读到该行时的版本（字段级冲突判定）；任务 / 流程写回用 'latest'，按最新行执行、不判冲突（用户命令带 'latest' 是 INVALID_COMMAND）。 */
export type BaseVersion = number | 'latest';
export type DataCommand =
  | { op: 'patchDraftFields'; fields: Partial<Draft>; baseVersion: BaseVersion }
  | { op: 'patchShot'; shotId: string; fields: Partial<Shot>; baseVersion: BaseVersion }
  | { op: 'mergeShots'; shotId: string; sourceShotId: string; fields: Partial<Shot>; baseVersion: number; sourceBaseVersion: number }
  | { op: 'patchEpisode'; episodeId: string; fields: Partial<Episode>; baseVersion: BaseVersion }
  | { op: 'patchAsset'; assetId: string; fields: Partial<Asset>; baseVersion: BaseVersion }
  | { op: 'patchScene'; sceneId: string; fields: Partial<ScriptScene>; baseVersion: BaseVersion }
  | { op: 'patchShotBoard'; shotId: string; boardId: number; fields: Partial<ShotStoryboardBoardImage>; baseVersion: BaseVersion }
  | { op: 'replaceShotBoards'; shotId: string; boards: Array<Pick<ShotStoryboardBoardImage, 'boardId' | 'imagePrompt' | 'purpose' | 'frameCount' | 'anchorBeatIds' | 'coverageStartBeatId' | 'coverageEndBeatId'>>; baseVersion: BaseVersion }
  | { op: 'insertShots'; episodeId: string; shots: Shot[]; afterShotId: string | null }
  | { op: 'replaceEpisodeShots'; episodeId: string; shots: Shot[]; scope: ShotReplaceScope }
  | { op: 'deleteShots'; shotIds: string[] }
  | { op: 'reorderShots'; episodeId: string; orderedIds: string[] }
  | { op: 'insertEpisodes'; episodes: Episode[]; afterEpisodeId: string | null }
  | { op: 'deleteEpisodes'; episodeIds: string[] }
  | { op: 'reorderEpisodes'; orderedIds: string[] }
  | { op: 'insertAssets'; assets: Asset[] }
  | { op: 'deleteAssets'; assetIds: string[] }
  | { op: 'addAssetAliases'; assetId: string; aliases: string[] }
  | { op: 'removeAssetAliases'; assetId: string; aliases: string[] }
  | ({ op: 'addAssetAudio'; assetId: string; audio: NonNullable<Asset['audios']>[number] & { id: string } } & ({ baseVersion: 'latest'; select?: boolean } | { baseVersion: number; select: boolean }))
  | { op: 'appendAssetState'; assetId: string; state: Record<string, unknown>; slot?: 'states' | 'sceneStates' }
  | { op: 'approveAssetImage'; assetId: string; variantId: string | null }
  | { op: 'replaceScenes'; scenes: ScriptScene[] }
  | { op: 'insertScenes'; scenes: ScriptScene[]; afterSceneId: string | null }
  | { op: 'deleteScenes'; sceneIds: string[] }
  | { op: 'updateEpisodeManifest'; episodeId: string; setAiAssetIds?: string[]; include?: string[]; exclude?: string[]; unmatched?: unknown[] }
  | { op: 'resetEpisodeDerivations'; episodeId: string; baseVersion: BaseVersion }
  /** replaceSource：同一 source 的旧候选先摘掉（去水印版取代上一版）。 */
  | { op: 'addMediaVariant'; subject: MediaSubject; variant: MediaVariant; select: boolean; replaceSource?: string }
  | { op: 'selectMediaVariant' | 'removeMediaVariant'; subject: MediaSubject; variantId: string }
  | { op: 'markMediaSeen'; entries: Array<{ subject: MediaSubject; variantIds: string[] }> }
  | { op: 'patchCanvasNodeData'; nodeId: string; fields: Record<string, unknown>; baseVersion?: number }
  | { op: 'replaceCanvasGraph'; canvas: NonNullable<Draft['canvasState']>; baseVersion: number };
export type DraftApplyRequest = { commandId: string; draftId: string; imageEditorProjectId?: never; origin: CommandOrigin; commands: DataCommand[]; onMissing?: 'error' | 'skip' };
export type ImageEditorApplyRequest = { commandId: string; imageEditorProjectId: string; draftId?: never; origin: CommandOrigin; commands: ImageEditorCommand[]; onMissing?: never };
export type ApplyRequest = DraftApplyRequest | ImageEditorApplyRequest;
export type ChangedRow = { entity: DataEntity | 'imageEditorProject'; id: string; version: number; fields: string[]; sortOrder?: number; deleted?: true; data?: unknown };
export type ApplyResult = { commandId: string; replayed: boolean; rows: ChangedRow[]; skipped?: Array<{ op: string; reason: 'ENTITY_NOT_FOUND' | 'DRAFT_NOT_FOUND' }> };
export type ApplyResponse = { ok: true; result: ApplyResult } | { ok: false; error: { name: string; message: string; code?: string; details: Record<string, unknown> } };
export type DataChangedEvent = Omit<ChangedRow, 'data'> & { draftId?: string; imageEditorProjectId?: string; commandId: string };
export type DataCommandErrorCode = 'FIELD_CONFLICT' | 'ENTITY_NOT_FOUND' | 'DRAFT_NOT_FOUND' | 'FIELD_REJECTED' | 'INVALID_COMMAND' | 'HOST_NOT_READY';

export class DataCommandError extends Error {
  constructor(public code: DataCommandErrorCode, message: string, public details: Record<string, unknown> = {}) {
    super(message);
    this.name = 'DataCommandError';
    Object.assign(this, details);
  }
}
