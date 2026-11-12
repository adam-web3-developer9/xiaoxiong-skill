import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { EventEmitter } from "node:events";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { AgentClient, ALL_SCOPES } from "../src/client.mjs";
test("Node client talks to actual Rust Session/Auth/routes over test stdio", async (t) => {
  const binary = process.env.XIAOXIONG_NATIVE_FIXTURE;
  if (!binary) {
    t.skip(
      "Set XIAOXIONG_NATIVE_FIXTURE to the compiled test-only protocol_stdio example",
    );
    return;
  }
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-rust-contract-"));
  await fs.writeFile(
    path.join(root, "agent-v2-connection.json"),
    JSON.stringify({
      protocol: 2,
      endpoint: path.join(root, "test-stream"),
      dataRoot: root,
    }),
  );
  let child;
  const factory = () => {
    child = spawn(binary, [root], { stdio: ["pipe", "pipe", "pipe"] });
    const socket = new EventEmitter();
    socket.destroyed = false;
    socket.setEncoding = (encoding) => child.stdout.setEncoding(encoding);
    socket.write = (data, callback) => child.stdin.write(data, callback);
    socket.destroy = () => {
      socket.destroyed = true;
      child.kill();
    };
    child.stdout.on("data", (data) => socket.emit("data", data));
    child.on("error", (e) => socket.emit("error", e));
    child.on("spawn", () => socket.emit("connect"));
    child.on("close", () => {
      socket.destroyed = true;
      socket.emit("close");
    });
    return socket;
  };
  const client = new AgentClient({
    dataRoot: root,
    home: path.join(root, "state"),
    socketFactory: factory,
  });
  t.after(async () => {
    client.close();
    await fs.rm(root, { recursive: true, force: true });
  });
  let code;
  const session = await client.login({
    scopes: ALL_SCOPES,
    draftIds: ["draft-1"],
    onPending: (p) => (code = p.userCode),
  });
  assert.equal(code.length, 8);
  assert(session.connected);
  const caps = await client.call("capabilities");
  assert.equal(caps.taskActions.length, 48);
  assert.equal(caps.credentialRead, false);
  assert.deepEqual(
    (await client.call("draft.list")).map((d) => d.id),
    ["draft-1"],
  );
  const draft = await client.call("draft.get", { draftId: "draft-1" });
  assert.equal(draft.episodes[0].text, "测试正文");
  await assert.rejects(
    client.call("draft.get", { draftId: "other-draft" }),
    (e) => e.code === "AGENT_DRAFT_DENIED",
  );
  const write = await client.call("draft.apply", {
    commandId: "native-command",
    draftId: "draft-1",
    commands: [],
  });
  assert.equal(write.commandId, "native-command");
  assert(
    !JSON.stringify(await client.call("account.status")).includes(
      "FIXTURE_SECRET",
    ),
  );
  assert.deepEqual(
    await client.call("channels.create", {
      providerId: "new",
      patch: { name: "New" },
      credentials: { apiKey: "FIXTURE_SECRET_MUST_NOT_LEAK" },
    }),
    { success: true, providerId: "new" },
  );
  await client.logout();
});
