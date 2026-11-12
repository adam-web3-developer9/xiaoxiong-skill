import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { discover } from "../src/discovery.mjs";
async function create(t) {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), "xx-discovery-"));
  t.after(() => fs.rm(base, { recursive: true, force: true }));
  const tmp = path.join(base, "temporary");
  await fs.mkdir(tmp);
  return { base, tmp };
}
async function instance(base, tmp, id) {
  const root = path.join(base, id);
  await fs.mkdir(root);
  const descriptor = {
    protocol: 2,
    schemaVersion: 1,
    endpoint:
      process.platform === "win32"
        ? `\\\\.\\pipe\\xx-discovery-${process.pid}-${id}`
        : path.join(root, "app.sock"),
    dataRoot: root,
    pid: process.pid,
    instanceId: id,
  };
  await fs.writeFile(
    path.join(root, "agent-v2-connection.json"),
    JSON.stringify(descriptor),
  );
  await fs.writeFile(
    path.join(tmp, `aistudio-agent-v2-${process.pid}-${id}.json`),
    JSON.stringify(descriptor),
    { mode: 0o600 },
  );
  return descriptor;
}
test("public markers automatically find a custom application data root", async (t) => {
  const { base, tmp } = await create(t);
  const d = await instance(base, tmp, "abc123");
  assert.equal(
    (await discover({ roots: [], discoveryDirectory: tmp })).dataRoot,
    d.dataRoot,
  );
});
test("marker must match current root descriptor instance, and explicit root does not scan markers", async (t) => {
  const { base, tmp } = await create(t);
  const d = await instance(base, tmp, "abc123");
  await fs.writeFile(
    path.join(d.dataRoot, "agent-v2-connection.json"),
    JSON.stringify({ ...d, instanceId: "replacement" }),
  );
  await assert.rejects(
    discover({ roots: [], discoveryDirectory: tmp }),
    (e) => e.code === "APP_NOT_FOUND",
  );
  assert.equal(
    (
      await discover({
        dataRoot: d.dataRoot,
        roots: [],
        discoveryDirectory: tmp,
      })
    ).instanceId,
    "replacement",
  );
});
test("multiple live applications require an explicit profile choice", async (t) => {
  const { base, tmp } = await create(t);
  await instance(base, tmp, "abc123");
  await instance(base, tmp, "def456");
  await assert.rejects(
    discover({ roots: [], discoveryDirectory: tmp }),
    (e) =>
      e.code === "MULTIPLE_APPLICATIONS" && e.details.profiles.length === 2,
  );
});
