import fs from "node:fs/promises";
import path from "node:path";
import { assert } from "./errors.mjs";
export function scriptFiles(draft, { episodeIds, source = false } = {}) {
  assert(draft && Array.isArray(draft.episodes), "Draft has no episodes");
  const selected = episodeIds ? new Set(episodeIds) : null;
  const known = new Set(draft.episodes.map((e) => e.id));
  if (selected)
    for (const id of selected)
      assert(known.has(id), `Episode not found: ${id}`);
  return draft.episodes.flatMap((e, index) =>
    selected && !selected.has(e.id)
      ? []
      : [
          {
            episodeId: e.id,
            filename: `${String(index + 1).padStart(3, "0")}-${
              String(e.title || e.id)
                .replace(/[<>:"/\\|?*\x00-\x1F]/g, "_")
                .slice(0, 100)
                .replace(/[. ]+$/, "") || "episode"
            }.txt`,
            text: String(
              source ? (e.sourceText ?? e.text ?? "") : (e.text ?? ""),
            ),
          },
        ],
  );
}
export async function exportScripts(
  client,
  { draftId, episodeIds, source = false },
  output,
) {
  assert(draftId, "draftId is required");
  assert(output, "--output directory is required for script export");
  const draft = await client.call("draft.get", { draftId });
  const files = scriptFiles(draft, { episodeIds, source });
  assert(files.length, "No episodes selected");
  await fs.mkdir(output, { recursive: true });
  const written = [];
  try {
    for (const file of files) {
      const target = path.join(output, file.filename);
      await fs.writeFile(target, file.text, { flag: "wx", mode: 0o600 });
      written.push(target);
    }
  } catch (error) {
    error.details = { writtenFiles: written };
    throw error;
  }
  return {
    draftId,
    files: written,
    count: files.length,
    source: source ? "sourceText" : "text",
    note: "Exports committed application data. Unsaved editor buffers are not included.",
  };
}
