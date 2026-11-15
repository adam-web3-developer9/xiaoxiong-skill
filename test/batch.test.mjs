import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";
import { prepareBatch } from "../src/batch.mjs";
import { main } from "../bin/xiaoxiong.mjs";
import { serveMcp } from "../src/mcp.mjs";

const selection = {
  draftId: "draft-1",
  episodes: [{ id: "episode-2", title: "第二集" }, { id: "episode-1", title: "第一集" }],
  stageKeys: ["shotVideo", "videoPrompt"],
  storyboardMode: "greedy30",
};
function input(value) {
  const stream = new PassThrough();
  stream.end(JSON.stringify(value));
  return stream;
}

test("new batch preserves episode selection, initializes states and leaves scheduling to the app", () => {
  const original = structuredClone(selection);
  const { draftId, run } = prepareBatch(selection);
  assert.equal(run.draftId, draftId);
  assert.deepEqual(run.episodeIds, ["episode-2", "episode-1"]);
  assert.deepEqual(run.stageKeys, ["videoPrompt", "shotVideo"]);
  assert.equal(run.storyboardDuration, 30);
  assert.equal(run.storyboardGreedy, true);
  assert.equal(run.storyboardForceMax, true);
  assert.equal(run.episodes["episode-2"].title, "第二集");
  assert.deepEqual(run.episodes["episode-1"].stages.shotVideo, { status: "idle", taskIds: [], attempt: 0 });
  assert.deepEqual(run.episodes["episode-1"].stages.storyboard, { status: "skipped", taskIds: [], attempt: 0 });
  assert.equal(Object.keys(run.episodes["episode-1"].stages).length, 8);
  assert.equal(run.retryCount, undefined);
  assert.equal(run.storyboardEnabled, undefined);
  assert.notEqual(prepareBatch(selection).run.runId, run.runId);
  assert.deepEqual(selection, original);
});

test("no-board selection skips board-only steps and all four storyboard modes keep their duration semantics", () => {
  const result = prepareBatch({ ...selection,
    stageKeys: ["imagePrompt", "shotImage", "videoPrompt", "shotVideo"],
    storyboardEnabled: false,
  });
  assert.deepEqual(result.run.stageKeys, ["videoPrompt", "shotVideo"]);
  assert.equal(result.run.storyboardEnabled, false);
  assert.equal(result.run.episodes["episode-1"].stages.shotImage.status, "skipped");
  for (const [mode, duration, greedy] of [
    ["max15", 15, false], ["max30", 30, false],
    ["greedy15", 15, true], ["greedy30", 30, true],
  ]) {
    const { run } = prepareBatch({ ...selection, storyboardMode: mode });
    assert.equal(run.storyboardDuration, duration);
    assert.equal(run.storyboardGreedy, greedy);
    assert.equal(run.storyboardForceMax, true);
  }
});

test("CLI prepares offline and its saved output feeds estimate/start without wrapping or changing runId", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-batch-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const file = path.join(root, "batch.json");
  const prepared = await main(["batch", "prepare", "--file", "-", "--output", file], {
    stdin: input(selection),
    clientFactory() { assert.fail("prepare must not connect to the app"); },
  });
  assert.equal(prepared.ok, true);
  const payload = JSON.parse(await fs.readFile(file, "utf8"));
  assert.equal(payload.ok, undefined);
  const calls = [];
  for (const action of ["estimate", "start"]) {
    const result = await main(["batch", action, "--file", file], {
      clientFactory: () => ({
        call: async (...args) => { calls.push(args); return { runId: args[1].run.runId }; },
        close() {},
      }),
    });
    assert.equal(result.ok, true);
  }
  assert.deepEqual(calls, [
    ["batch.estimate", payload, { write: false }],
    ["batch.start", payload, { write: true }],
  ]);
});

test("MCP exposes a usable batch run schema and controls the same app run", async () => {
  const calls = [], output = new PassThrough();
  let raw = "";
  output.on("data", (chunk) => { raw += chunk; });
  const payload = prepareBatch(selection);
  const client = {
    call: async (...args) => { calls.push(args); return { status: "running" }; },
    close() {},
  };
  const messages = [{ jsonrpc: "2.0", id: 1, method: "tools/list" }];
  const commands = [
    ["estimate", payload, false], ["start", payload, true],
    ...["progress", "pause", "resume", "stop"].map((action) => [action,
      { draftId: payload.draftId, runId: payload.run.runId }, !["progress"].includes(action)]),
    ["retryStage", { draftId: payload.draftId, runId: payload.run.runId, episodeId: "episode-1", stage: "shotVideo" }, true],
  ];
  for (const [action, args] of commands) messages.push({
    jsonrpc: "2.0", id: messages.length + 1, method: "tools/call",
    params: { name: `xiaoxiong_batch_${action}`, arguments: args },
  });
  const stream = new PassThrough();
  stream.end(messages.map((m) => JSON.stringify(m)).join("\n") + "\n");
  await serveMcp(client, { input: stream, output });
  const replies = raw.trim().split("\n").map(JSON.parse);
  const schema = replies[0].result.tools.find((t) => t.name === "xiaoxiong_batch_start").inputSchema;
  assert(schema.properties.run.required.includes("episodes"));
  assert(schema.properties.run.properties.stageKeys.items.enum.includes("shotVideo"));
  assert.equal(schema.properties.run.properties.episodes.additionalProperties.properties.stages.required.length, 8);
  assert.deepEqual(calls, commands.map(([action, args, write]) => [`batch.${action}`, args, { write }]));
  assert(replies.slice(1).every((reply) => reply.result.isError === false));
});
