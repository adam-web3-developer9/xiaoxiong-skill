// Pure local adapter. Source schema: aistudio@5423be21112b61b0728cf10a0bc9883e0e308581.
// This returns a reviewable request, never calls an app/provider or executes a Skill.
export function creativeToCommands(input, { commandId, draftId, windowId }) {
  const required = (value, label) => {
    if (typeof value !== "string" || !value.trim())
      throw new Error(`${label} must be a nonempty string`);
    return value;
  };
  for (const [k, v] of Object.entries({ commandId, draftId, windowId }))
    required(v, k);
  if (
    !input ||
    !Array.isArray(input.episodes) ||
    !Array.isArray(input.assets) ||
    !Array.isArray(input.shots)
  )
    throw new Error("Expected episodes, assets and shots arrays");
  const ids = new Set();
  const id = (value) => {
    required(value, "id");
    if (ids.has(value)) throw new Error(`Duplicate id: ${value}`);
    ids.add(value);
    return value;
  };
  const pick = (obj, keys) => {
    const unknown = Object.keys(obj).filter((k) => !keys.includes(k));
    if (unknown.length)
      throw new Error(`Unmapped fields: ${unknown.join(", ")}`);
    const result = {};
    for (const key of keys)
      if (obj[key] !== undefined) {
        if (key === "duration") {
          if (
            typeof obj[key] !== "number" ||
            !Number.isFinite(obj[key]) ||
            obj[key] <= 0
          )
            throw new Error("duration must be positive");
        } else if (key !== "storyboardBoards" && key !== "cutPlan")
          required(obj[key], key);
        result[key] = obj[key];
      }
    return result;
  };
  const episodes = input.episodes.map((e) => {
    const row = pick(e, ["id", "title", "text"]);
    id(row.id);
    required(row.title, "title");
    required(row.text, "text");
    return row;
  });
  const episodeIds = new Set(episodes.map((e) => e.id));
  const assets = input.assets.map((a) => {
    const row = pick(a, ["id", "name", "type", "description", "prompt"]);
    id(row.id);
    return row;
  });
  const shots = input.shots.map((s) => {
    const row = pick(s, [
      "id",
      "episodeId",
      "scriptContent",
      "originalText",
      "description",
      "visualDescription",
      "narration",
      "duration",
      "videoPrompt",
      "storyboardBoards",
      "cutPlan",
    ]);
    id(row.id);
    if (!episodeIds.has(row.episodeId))
      throw new Error("Shot must reference an episode in this import");
    required(row.scriptContent, "Shot scriptContent");
    if (row.duration === undefined) throw new Error("Shot duration is required");
    return row;
  });
  const commands = [];
  if (episodes.length)
    commands.push({ op: "insertEpisodes", episodes, afterEpisodeId: null });
  if (assets.length) commands.push({ op: "insertAssets", assets });
  for (const episode of episodes) {
    const group = shots.filter((s) => s.episodeId === episode.id);
    if (group.length)
      commands.push({
        op: "insertShots",
        episodeId: episode.id,
        shots: group,
        afterShotId: null,
      });
  }
  return {
    commandId,
    draftId,
    origin: { kind: "user", windowId },
    commands,
    onMissing: "error",
  };
}
