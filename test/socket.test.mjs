import { test } from "node:test";
import assert from "node:assert/strict";
import net from "node:net";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Transport } from "../src/transport.mjs";
test("real OS local socket framing (explicitly skipped when host forbids listen)", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-socket-"));
  const endpoint = path.join(root, "probe.sock");
  const server = net.createServer((s) => {
    s.setEncoding("utf8");
    let buffer = "";
    s.on("data", (c) => {
      buffer += c;
      let n;
      while ((n = buffer.indexOf("\n")) >= 0) {
        const m = JSON.parse(buffer.slice(0, n));
        buffer = buffer.slice(n + 1);
        s.write(
          JSON.stringify({ id: m.id, ok: true, result: { echo: m.params } }) +
            "\n",
        );
      }
    });
  });
  try {
    await new Promise((r, j) => {
      server.once("error", j);
      server.listen(endpoint, r);
    });
  } catch (e) {
    await fs.rm(root, { recursive: true, force: true });
    if (["EPERM", "EACCES"].includes(e.code)) {
      t.skip(`Local socket bind unavailable: ${e.code}`);
      return;
    }
    throw e;
  }
  const client = new Transport(endpoint);
  try {
    await client.connect();
    assert.deepEqual(await client.request("echo", { text: "小熊AI" }), {
      echo: { text: "小熊AI" },
    });
  } finally {
    client.close();
    await new Promise((r) => server.close(r));
    await fs.rm(root, { recursive: true, force: true });
  }
});
