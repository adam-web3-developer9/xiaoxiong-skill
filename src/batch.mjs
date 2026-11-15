import { randomUUID } from "node:crypto";
import { assert } from "./errors.mjs";

// AIStudio src/core/batch/pipeline.ts: createBatchRun (3.1.79).
// Scheduling, retries and task preparation remain application-owned.
export const BATCH_STAGES = [
  "assets", "assetImages", "storyboard", "imagePrompt",
  "videoPrompt", "shotImage", "shotVideo", "watermarkRemove",
];
export const STORYBOARD_MODES = {
  max15: { storyboardDuration: 15, storyboardForceMax: true, storyboardGreedy: false },
  max30: { storyboardDuration: 30, storyboardForceMax: true, storyboardGreedy: false },
  greedy15: { storyboardDuration: 15, storyboardForceMax: true, storyboardGreedy: true },
  greedy30: { storyboardDuration: 30, storyboardForceMax: true, storyboardGreedy: true },
};

/** Prepare a new batch request locally; this does not submit or estimate it. */
export function prepareBatch(config) {
  assert(config && typeof config === "object", "Batch configuration is required");
  assert(typeof config.draftId === "string" && config.draftId.trim(), "draftId is required");
  assert(Array.isArray(config.episodes) && config.episodes.length > 0, "episodes are required");
  for (const episode of config.episodes)
    assert(typeof episode?.id === "string" && episode.id.trim(), "Each episode needs its saved id");
  assert(new Set(config.episodes.map((e) => e.id)).size === config.episodes.length, "Episode IDs must be unique");
  assert(Array.isArray(config.stageKeys) && config.stageKeys.length > 0 &&
    config.stageKeys.every((s) => BATCH_STAGES.includes(s)), "stageKeys must contain supported batch stages");
  const mode = config.storyboardMode ?? "max15";
  assert(Object.hasOwn(STORYBOARD_MODES, mode), "storyboardMode must be max15, max30, greedy15 or greedy30");
  assert(config.storyboardEnabled === undefined || typeof config.storyboardEnabled === "boolean",
    "storyboardEnabled must be a boolean when supplied");
  const stageKeys = BATCH_STAGES.filter((stage) => config.stageKeys.includes(stage) &&
    (config.storyboardEnabled !== false || !["imagePrompt", "shotImage"].includes(stage)));
  const now = Date.now();
  return {
    draftId: config.draftId,
    run: {
      runId: `batch-${randomUUID()}`,
      draftId: config.draftId,
      status: "running",
      createdAt: now,
      updatedAt: now,
      episodeIds: config.episodes.map((e) => e.id),
      stageKeys,
      ...STORYBOARD_MODES[mode],
      ...(config.storyboardEnabled === undefined ? {} : { storyboardEnabled: config.storyboardEnabled }),
      episodes: Object.fromEntries(config.episodes.map((episode) => [episode.id, {
        episodeId: episode.id,
        title: episode.title || "未命名剧集",
        stages: Object.fromEntries(BATCH_STAGES.map((stage) => [stage, {
          status: stageKeys.includes(stage) ? "idle" : "skipped",
          taskIds: [],
          attempt: 0,
        }])),
      }])),
    },
  };
}

const savedId = { type: "string", minLength: 1 };
export const batchRunSchema = {
  type: "object",
  description: "New AIStudio batch run. Build with xiaoxiong batch prepare or the batch-assistant Skill reference. Preserve runId after submission; never replay an uncertain start.",
  properties: {
    runId: { ...savedId, description: "Stable ID for this new batch" },
    draftId: { ...savedId, description: "Must equal the outer draftId" },
    status: { const: "running" },
    createdAt: { type: "number" },
    updatedAt: { type: "number" },
    episodeIds: { type: "array", items: savedId, minItems: 1, uniqueItems: true },
    stageKeys: { type: "array", items: { enum: BATCH_STAGES }, uniqueItems: true },
    storyboardDuration: { enum: [15, 30] },
    storyboardForceMax: { type: "boolean" },
    storyboardGreedy: { type: "boolean" },
    storyboardEnabled: { type: "boolean" },
    episodes: {
      type: "object",
      description: "Map each selected episode ID to its initial state; include all eight stages, unselected stages skipped",
      additionalProperties: {
        type: "object",
        properties: {
          episodeId: savedId,
          title: { type: "string" },
          stages: {
            type: "object",
            properties: Object.fromEntries(BATCH_STAGES.map((stage) => [stage, {
              type: "object",
              properties: {
                status: { enum: ["idle", "skipped"] },
                taskIds: { type: "array", items: savedId },
                attempt: { type: "integer", minimum: 0 },
              },
              required: ["status", "taskIds", "attempt"],
            }])),
            required: BATCH_STAGES,
          },
        },
        required: ["episodeId", "title", "stages"],
      },
    },
  },
  required: ["runId", "draftId", "status", "createdAt", "updatedAt", "episodeIds",
    "stageKeys", "storyboardDuration", "storyboardForceMax", "episodes"],
  additionalProperties: true,
};
