import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  discoverSkills,
  readSkill,
  readSkillResource,
  inspectSkill,
  installSkill,
} from "../src/skills.mjs";
async function fixture(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-skills-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  return root;
}
async function packageAt(root, name = "demo") {
  const dir = path.join(root, name);
  await fs.mkdir(path.join(dir, "references"), { recursive: true });
  await fs.writeFile(
    path.join(dir, "SKILL.md"),
    `---\nname: ${name}\ndescription: >\n  Write creative scenes\n  with continuity\nmetadata:\n  version: 1\n---\nRead references/guide.md lazily.\n`,
  );
  await fs.writeFile(path.join(dir, "references/guide.md"), "Reference text");
  return dir;
}
test("discovers multiple packages, deduplicates roots and lazily reads resources", async (t) => {
  const root = await fixture(t),
    a = await packageAt(root),
    b = await packageAt(root, "other");
  const all = await discoverSkills([root, a]);
  assert.equal(all.length, 2);
  assert.equal(all[0].description, "Write creative scenes with continuity");
  assert.equal(all[0].body, undefined);
  assert.equal(
    await readSkillResource(b, "references/guide.md"),
    "Reference text",
  );
  assert.match((await readSkill(a)).frontmatter, /version: 1/);
});
test("inspection surfaces scripts and unavailable tools without execution", async (t) => {
  const root = await fixture(t),
    a = await packageAt(root);
  await fs.mkdir(path.join(a, "scripts"));
  await fs.writeFile(path.join(a, "scripts/run.sh"), "exit 99");
  await fs.appendFile(path.join(a, "SKILL.md"), "Use mcp__vendor__generate.");
  const report = await inspectSkill(a);
  assert.equal(report.compatibility.runnable, "not-verified");
  assert.deepEqual(report.compatibility.externalScripts, ["scripts/run.sh"]);
  assert.ok(
    report.compatibility.unknownTools.includes("mcp__vendor__generate"),
  );
});
test("installs local package without running scripts and refuses overwrite", async (t) => {
  const root = await fixture(t),
    a = await packageAt(root),
    target = path.join(root, "installed");
  const result = await installSkill(a, target);
  assert.equal(result.executedScripts, false);
  assert.equal(
    await readSkillResource(target, "references/guide.md"),
    "Reference text",
  );
  await assert.rejects(installSkill(a, target), { code: "EEXIST" });
  await assert.rejects(installSkill(a, path.join(a, "nested")), /overlap/);
});
test("rejects traversal, absolute paths and symbolic links on reads and install", async (t) => {
  const root = await fixture(t),
    a = await packageAt(root);
  await fs.writeFile(path.join(root, "outside"), "secret");
  for (const p of [
    "../outside",
    "/etc/passwd",
    "C:\\secret",
    "references\\guide.md",
  ])
    await assert.rejects(readSkillResource(a, p));
  const outsideDirectory = path.join(root, "outside-directory");
  await fs.mkdir(outsideDirectory);
  await fs.writeFile(path.join(outsideDirectory, "content"), "secret");
  await fs.symlink(
    outsideDirectory,
    path.join(a, "references/leak"),
    process.platform === "win32" ? "junction" : "dir",
  );
  await assert.rejects(readSkillResource(a, "references/leak/content"), /Symbolic/);
  await assert.rejects(
    installSkill(a, path.join(root, "installed")),
    /Symbolic/,
  );
});
test("missing, malformed, quoted and oversized metadata fail visibly", async (t) => {
  const root = await fixture(t),
    a = await packageAt(root);
  await fs.writeFile(path.join(a, "SKILL.md"), "plain text");
  assert.ok((await readSkill(a)).warnings.includes("Missing name"));
  await fs.writeFile(
    path.join(a, "SKILL.md"),
    "---\nname: \"quoted\"\ndescription: 'It''s useful'\n---\nbody",
  );
  assert.equal((await readSkill(a)).description, "It's useful");
  await fs.writeFile(
    path.join(a, "references/huge"),
    Buffer.alloc(2 * 1024 * 1024 + 1),
  );
  await assert.rejects(readSkillResource(a, "references/huge"), /2 MiB/);
  assert.deepEqual(await discoverSkills(path.join(root, "missing")), []);
});
test("bundled operator skill can be loaded and inspected", async () => {
  const result = await inspectSkill(
    fileURLToPath(new URL("../skills/xiaoxiong-ai", import.meta.url)),
  );
  assert.equal(result.name, "xiaoxiong-ai");
  assert.equal(result.compatibility.externalScripts.length, 0);
  assert.ok(result.resources.includes("references/operations.md"));
});
test("creative adapter uses verified creation commands and rejects lossy mappings", async () => {
  const { creativeToCommands } = await import(
    "../examples/creative-to-commands.mjs"
  );
  const options = { commandId: "c", draftId: "d", windowId: "w" };
  const input = {
    episodes: [{ id: "e", title: "Episode 1", text: "A story" }],
    assets: [{ id: "a", name: "Hero", type: "character", prompt: "portrait" }],
    shots: [
      {
        id: "s",
        episodeId: "e",
        scriptContent: "Hero arrives",
        description: "Hero arrives",
        duration: 5,
      },
    ],
  };
  const result = creativeToCommands(input, options);
  assert.deepEqual(
    result.commands.map((c) => c.op),
    ["insertEpisodes", "insertAssets", "insertShots"],
  );
  assert.equal(result.commandId, "c");
  assert.throws(
    () =>
      creativeToCommands(
        {
          ...input,
          shots: [
            {
              id: "s",
              episodeId: "missing",
              scriptContent: "Hero arrives",
              duration: 5,
            },
          ],
        },
        options,
      ),
    /reference/,
  );
  assert.throws(
    () =>
      creativeToCommands(
        { ...input, assets: [{ id: "a", unknown: "lost" }] },
        options,
      ),
    /Unmapped/,
  );
});

test("creative adapter accepts shot content and seconds without production metadata", async () => {
  const { creativeToCommands } = await import(
    "../examples/creative-to-commands.mjs"
  );
  const shot = {
    id: "s",
    episodeId: "e",
    scriptContent: "林舟推门进屋，放下行李。",
    duration: 8,
  };
  const result = creativeToCommands(
    {
      episodes: [{ id: "e", title: "第一集", text: "林舟回家。" }],
      assets: [],
      shots: [shot],
    },
    { commandId: "c", draftId: "d", windowId: "agent:test" },
  );
  assert.deepEqual(result.commands[1].shots, [shot]);
  for (const missing of ["scriptContent", "duration"]) {
    const incomplete = { ...shot };
    delete incomplete[missing];
    assert.throws(
      () =>
        creativeToCommands(
          {
            episodes: [{ id: "e", title: "第一集", text: "林舟回家。" }],
            assets: [],
            shots: [incomplete],
          },
          { commandId: "c", draftId: "d", windowId: "agent:test" },
        ),
      missing === "duration" ? /duration is required/ : /nonempty string/,
    );
  }
});

test("creative adapter preserves creator prompts and optional shot metadata", async () => {
  const { creativeToCommands } = await import(
    "../examples/creative-to-commands.mjs"
  );
  const shot = {
    id: "s",
    episodeId: "e",
    scriptContent: "林舟在门口停住。",
    duration: 9,
    videoPrompt: "固定镜头，林舟轻推木门，停顿两秒后抬头。",
    storyboardBoards: [
      { boardId: 1, imagePrompt: "暖光门厅，林舟半身，保留门框。", frameCount: 1 },
    ],
    cutPlan: {
      sceneGoal: "表现归家的犹疑",
      startState: "门外停步",
      endState: "抬头看向屋内",
    },
  };
  const original = structuredClone(shot);
  const result = creativeToCommands(
    {
      episodes: [{ id: "e", title: "第一集", text: "林舟回家。" }],
      assets: [],
      shots: [shot],
    },
    { commandId: "c", draftId: "d", windowId: "agent:test" },
  );
  assert.deepEqual(result.commands[1].shots, [original]);
  assert.deepEqual(shot, original);
});
