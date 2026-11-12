import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { METHODS } from "../src/catalog.mjs";
import { READ_METHODS } from "../src/client.mjs";
import { scriptFiles } from "../src/export.mjs";
test("CLI/MCP catalog is exactly implemented application v2 methods", async () => {
  const rust = await fs.readFile(
    new URL(
      "../integration/aistudio/overlay/crates/aistudio-agent-data/src/v2_routes.rs",
      import.meta.url,
    ),
    "utf8",
  );
  const block = rust.match(/const METHODS:[\s\S]*?=\s*&\[([\s\S]*?)\];/)[1];
  const methods = [...block.matchAll(/\("([^"]+)",\s*"[^"]+"\)/g)].map(
    (m) => m[1],
  );
  assert.deepEqual(
    [...new Set(Object.values(METHODS).map((v) => v.method))].sort(),
    methods.sort(),
  );
  for (const v of Object.values(METHODS))
    assert.equal(
      READ_METHODS.has(v.method),
      !!v.read,
      `method safety mismatch ${v.method}`,
    );
});
test("script export preserves episode order/text and uses safe filenames", () => {
  const files = scriptFiles(
    {
      episodes: [
        { id: "e1", title: "../中文", text: "正文1", sourceText: "源1" },
        { id: "e2", title: "结尾", text: "正文2" },
      ],
    },
    { episodeIds: ["e2", "e1"] },
  );
  assert.deepEqual(
    files.map((f) => f.episodeId),
    ["e1", "e2"],
  );
  assert.equal(files[0].text, "正文1");
  assert(!files[0].filename.includes("/"));
  assert.throws(() =>
    scriptFiles({ episodes: [] }, { episodeIds: ["missing"] }),
  );
});
