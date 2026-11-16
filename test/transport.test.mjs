import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import { AgentClient } from "../src/client.mjs";
import { Transport } from "../src/transport.mjs";
import { discover } from "../src/discovery.mjs";
import { credentialPath, readReceipt } from "../src/storage.mjs";
async function fixture(t, handler) {
  const { EventEmitter } = await import("node:events");
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-agent-"));
  const socketPath =
    process.platform === "win32"
      ? `\\\\.\\pipe\\${path.basename(root)}`
      : path.join(root, "app.sock");
  const calls = [];
  const connections = [];
  const sockets = new Set();
  const socketFactory = (endpoint) => {
    connections.push(endpoint);
    const socket = new EventEmitter();
    socket.destroyed = false;
    socket.setEncoding = () => {};
    socket.destroy = () => {
      socket.destroyed = true;
      socket.emit("close");
      sockets.delete(socket);
    };
    socket.write = (data, callback) => {
      const m = JSON.parse(data);
      calls.push(m);
      queueMicrotask(async () => {
        try {
          const result = await handler(m, socket);
          if (result !== undefined && !socket.destroyed)
            socket.emit(
              "data",
              JSON.stringify({ id: m.id, ok: true, result }) + "\n",
            );
        } catch (e) {
          if (!socket.destroyed)
            socket.emit(
              "data",
              JSON.stringify({
                id: m.id,
                ok: false,
                error: {
                  code: e.code || "FAIL",
                  message: e.message,
                  details: e.details,
                },
              }) + "\n",
            );
        }
        callback?.();
      });
    };
    sockets.add(socket);
    queueMicrotask(() => socket.emit("connect"));
    return socket;
  };
  await fs.writeFile(
    path.join(root, "agent-v2-connection.json"),
    JSON.stringify({
      protocol: 2,
      endpoint: socketPath,
      dataRoot: root,
      instanceId: "test-app",
    }),
  );
  t.after(async () => {
    for (const s of sockets) s.destroy();
    await fs.rm(root, { recursive: true, force: true });
  });
  return {
    root,
    calls,
    connections,
    socketPath,
    home: path.join(root, "state"),
    socketFactory,
  };
}
function authHandler(extra = () => ({})) {
  let approved = false;
  let bound = false;
  return {
    approve: () => {
      approved = true;
    },
    handle(m, socket) {
      if (m.method === "hello") return { protocol: 2, schemaVersion: 1 };
      if (m.method === "auth.request")
        return {
          requestId: "pair",
          pollSecret: "poll-secret-long",
          userCode: "BEAR-1234",
          expiresAt: Date.now() + 3000,
          intervalMs: 100,
        };
      if (m.method === "auth.poll") {
        assert.equal(m.params.pollSecret, "poll-secret-long");
        return approved
          ? {
              sessionId: "session",
              token: "private-session-token",
              scopes: ["read", "edit", "channels.write"],
              draftIds: ["draft"],
              expiresAt: Date.now() + 10000,
            }
          : { status: "pending" };
      }
      if (m.method === "auth.resume") {
        assert.equal(m.params.token, "private-session-token");
        bound = true;
        return { sessionId: "session" };
      }
      if (m.method === "auth.logout") return { revoked: true };
      if (!bound || m.token !== "private-session-token")
        throw Object.assign(new Error("Not authorized"), {
          code: "AGENT_UNAUTHORIZED",
        });
      return extra(m, socket);
    },
  };
}
test("application pairing waits; credential never reaches status and is stored privately", async (t) => {
  const app = authHandler((m) => ({ id: m.params.draftId }));
  const f = await fixture(t, app.handle);
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  assert.equal((await c.status()).connected, false);
  setTimeout(() => app.approve(), 160);
  let shown;
  const session = await c.login({
    draftIds: ["draft"],
    onPending: (p) => {
      shown = p;
    },
  });
  assert.equal(session.connected, true);
  assert.equal(shown.userCode, "BEAR-1234");
  assert(!JSON.stringify(shown).includes("poll-secret"));
  assert(!JSON.stringify(session).includes("private-session-token"));
  assert.deepEqual(await c.call("draft.get", { draftId: "draft" }), {
    id: "draft",
  });
  assert.equal(f.calls.filter((m) => m.method === "auth.resume").length, 1);
  if (process.platform !== "win32")
    assert.equal(
      (await fs.stat(credentialPath(f.home, f.root))).mode & 0o777,
      0o600,
    );
  c.close();
  const second = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  assert.equal((await second.status()).connected, true);
  await second.logout();
  second.close();
  assert.equal(
    await fs.readFile(credentialPath(f.home, f.root)).catch((e) => e.code),
    "ENOENT",
  );
});
test("lost write response is unconfirmed, journaled, and never resent", async (t) => {
  const app = authHandler((m, socket) => {
    socket.destroy();
  });
  app.approve();
  const f = await fixture(t, app.handle);
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  await c.login();
  await assert.rejects(
    c.call("draft.apply", {
      commandId: "cmd-1",
      draftId: "draft",
      commands: [],
    }),
    (e) => e.code === "AGENT_WRITE_UNCONFIRMED",
  );
  assert.equal(f.calls.filter((m) => m.method === "draft.apply").length, 1);
  assert.equal((await readReceipt(f.home, "cmd-1")).status, "unconfirmed");
});
test("channel secrets absent from results, error messages and receipts", async (t) => {
  let shouldFail = false;
  const app = authHandler((m) => {
    if (shouldFail)
      throw Object.assign(new Error("provider rejected a-very-secret-key"), {
        code: "CHANNEL_FAILED",
        details: { apiKey: "a-very-secret-key" },
      });
    return {
      id: "p",
      apiKey: "a-very-secret-key",
      nested: { privateKey: "a-very-secret-key" },
      label: "a-very-secret-key",
    };
  });
  app.approve();
  const f = await fixture(t, app.handle);
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  await c.login();
  const params = {
    requestId: "channel-1",
    providerId: "p",
    credentials: { apiKey: "a-very-secret-key" },
  };
  const result = await c.call("channels.update", params);
  assert(!JSON.stringify(result).includes("a-very-secret-key"));
  assert(
    !JSON.stringify(await readReceipt(f.home, "channel-1")).includes(
      "a-very-secret-key",
    ),
  );
  shouldFail = true;
  await assert.rejects(
    c.call("channels.update", { ...params, requestId: "channel-2" }),
    (e) =>
      !JSON.stringify({ message: e.message, details: e.details }).includes(
        "a-very-secret-key",
      ),
  );
});
test("legacy descriptor never grants standalone client access", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "xx-old-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(
    path.join(root, "agent-data-connection.json"),
    JSON.stringify({ token: "do-not-read" }),
  );
  await assert.rejects(
    discover({ dataRoot: root }),
    (e) => e.code === "APP_UPGRADE_REQUIRED",
  );
});
test("transport keeps concurrent replies correlated", async (t) => {
  const f = await fixture(t, async (m) => {
    await new Promise((r) => setTimeout(r, m.params.delay));
    return m.params.value;
  });
  const c = new Transport(f.socketPath, { socketFactory: f.socketFactory });
  await c.connect();
  t.after(() => c.close());
  const values = await Promise.all([
    c.request("test", { delay: 30, value: 1 }),
    c.request("test", { delay: 1, value: 2 }),
  ]);
  assert.deepEqual(values, [1, 2]);
});
test("explicit RPC failures preserve uncertainty distinction", async (t) => {
  const f = await fixture(t, () => {
    throw Object.assign(new Error("field was edited"), {
      code: "FIELD_CONFLICT",
    });
  });
  const c = new Transport(f.socketPath, { socketFactory: f.socketFactory });
  await c.connect();
  t.after(() => c.close());
  await assert.rejects(
    c.request("draft.apply", { commandId: "c" }, { write: true }),
    (e) => e.code === "FIELD_CONFLICT",
  );
});

test("a new connection resumes cached grant without resubmitting the previous write", async (t) => {
  const app = authHandler((m) => ({ id: m.params.draftId }));
  app.approve();
  const f = await fixture(t, app.handle);
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  await c.login();
  c.transport.socket.destroy();
  assert.deepEqual(await c.call("draft.get", { draftId: "draft" }), {
    id: "draft",
  });
  assert.equal(f.calls.filter((m) => m.method === "auth.resume").length, 2);
});

test("application restart rediscovers its endpoint and resumes a persistent grant without pairing or replay", async (t) => {
  const app = authHandler((m) => ({ id: m.params.draftId }));
  app.approve();
  const f = await fixture(t, (m, socket) => {
    const result = app.handle(m, socket);
    return m.method === "auth.resume"
      ? {
          ...result,
          scopes: ["read", "edit"],
          draftIds: ["draft"],
          expiresAt: null,
        }
      : result;
  });
  const options = {
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  };
  const c = new AgentClient(options);
  t.after(() => c.close());
  await c.login();
  await c.call("draft.apply", {
    commandId: "before-restart",
    draftId: "draft",
    commands: [],
  });
  c.transport.socket.destroy();
  const endpoint = `${f.socketPath}-restarted`;
  await fs.writeFile(
    path.join(f.root, "agent-v2-connection.json"),
    JSON.stringify({
      protocol: 2,
      endpoint,
      dataRoot: f.root,
      instanceId: "restarted-app",
    }),
  );
  const status = await c.status();
  assert.equal(status.connected, true);
  assert.equal(status.expiresAt, null);
  assert.deepEqual(status.scopes, ["read", "edit"]);
  assert.deepEqual(f.connections, [f.socketPath, endpoint]);
  assert.equal(c.descriptor.instanceId, "restarted-app");
  c.close();
  const second = new AgentClient(options);
  t.after(() => second.close());
  assert.equal((await second.status()).expiresAt, null);
  const saved = JSON.parse(await fs.readFile(credentialPath(f.home, f.root), "utf8"));
  assert.equal(saved.expiresAt, null);
  assert.equal(saved.token, "private-session-token");
  assert.equal(f.calls.filter((m) => m.method === "auth.request").length, 1);
  assert.equal(f.calls.filter((m) => m.method === "auth.resume").length, 3);
  assert.equal(f.calls.filter((m) => m.method === "draft.apply").length, 1);
});

test("status revalidates an open connection and clears a revoked grant without pairing or replay", async (t) => {
  const app = authHandler(() => ({}));
  app.approve();
  let revoked = false;
  const f = await fixture(t, (m, socket) => {
    if (m.method === "auth.resume" && revoked)
      throw Object.assign(new Error("Connection was revoked"), {
        code: "AGENT_UNAUTHORIZED",
      });
    return app.handle(m, socket);
  });
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  await c.login();
  assert.equal((await c.status()).connected, true);
  revoked = true;
  const status = await c.status();
  assert.equal(status.connected, false);
  assert.equal(status.reason, "AGENT_UNAUTHORIZED");
  assert.equal(c.transport.token, undefined);
  assert.equal(
    await fs.readFile(credentialPath(f.home, f.root)).catch((e) => e.code),
    "ENOENT",
  );
  await assert.rejects(c.call("draft.get", { draftId: "draft" }), {
    code: "AUTH_REQUIRED",
  });
  assert.equal(f.connections.length, 1);
  assert.equal(f.calls.filter((m) => m.method === "auth.resume").length, 3);
  assert.equal(f.calls.filter((m) => m.method === "auth.request").length, 1);
  assert.equal(f.calls.filter((m) => m.method === "draft.get").length, 0);
});

test("receipt storage failure after confirmation never changes success into failure", async (t) => {
  let home;
  const app = authHandler(async () => {
    await fs.rm(path.join(home, "receipts"), { recursive: true, force: true });
    await fs.writeFile(path.join(home, "receipts"), "not-a-directory");
    return { confirmed: true };
  });
  app.approve();
  const f = await fixture(t, app.handle);
  home = f.home;
  const c = new AgentClient({
    dataRoot: f.root,
    home: f.home,
    socketFactory: f.socketFactory,
  });
  t.after(() => c.close());
  await c.login();
  assert.deepEqual(
    await c.call("draft.apply", {
      commandId: "known-confirmed",
      draftId: "draft",
      commands: [],
    }),
    { confirmed: true },
  );
  assert.equal(c.warnings[0].code, "RECEIPT_SAVE_FAILED");
  assert.equal(f.calls.filter((m) => m.method === "draft.apply").length, 1);
});
