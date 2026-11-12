import { test } from "node:test";
import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { main } from "../bin/xiaoxiong.mjs";
import { serveMcp } from "../src/mcp.mjs";
import { redact } from "../src/errors.mjs";
function fake() {
  const calls = [];
  return {
    calls,
    call: async (...args) => {
      calls.push(args);
      return { done: true };
    },
    close() {},
    status: async () => ({ connected: true }),
  };
}
test("CLI maps routes and UTF-8 JSON via stdin without shell secrets", async () => {
  const c = fake(),
    stdin = new PassThrough();
  stdin.end(
    "\uFEFF" +
      JSON.stringify({
        providerId: "p",
        credentials: { apiKey: "secret-only-stdin" },
      }),
  );
  const result = await main(["channels", "update", "--file", "-"], {
    stdin,
    clientFactory: () => c,
  });
  assert(result.ok);
  assert.equal(c.calls[0][0], "channels.update");
  assert.equal(c.calls[0][1].credentials.apiKey, "secret-only-stdin");
  assert(!JSON.stringify(result).includes("secret-only-stdin"));
});
test("CLI refuses unknown flags and missing mutation IDs", async () => {
  assert.equal(
    (await main(["--api-key", "oops"])).error.code,
    "INVALID_ARGUMENT",
  );
  const stdin = new PassThrough();
  stdin.end('{"draftId":"d","commands":[]}');
  const c = fake();
  assert.equal(
    (
      await main(["draft", "apply", "--file", "-"], {
        stdin,
        clientFactory: () => c,
      })
    ).error.code,
    "INVALID_ARGUMENT",
  );
  assert.equal(c.calls.length, 0);
});
test("redaction covers nested credential fields and literal submitted secrets", () => {
  assert.deepEqual(
    redact(
      {
        safe: 1,
        apiKey: "abc",
        config: { refreshToken: "def", value_base64: "xyz" },
        message: "abc-value",
      },
      ["abc-value"],
    ),
    {
      safe: 1,
      apiKey: "[redacted]",
      config: { refreshToken: "[redacted]", value_base64: "[redacted]" },
      message: "[redacted]",
    },
  );
});
test("MCP initialize/list/call obey stdio JSON RPC and no raw call tool", async () => {
  const input = new PassThrough(),
    output = new PassThrough();
  let raw = "";
  output.on("data", (c) => (raw += c));
  const c = fake();
  const promise = serveMcp(c, { input, output });
  input.end(
    [
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-03-26" },
      },
      { jsonrpc: "2.0", id: 2, method: "tools/list" },
      {
        jsonrpc: "2.0",
        id: 3,
        method: "tools/call",
        params: { name: "xiaoxiong_draft_get", arguments: { draftId: "d" } },
      },
    ]
      .map((v) => JSON.stringify(v))
      .join("\n") + "\n",
  );
  await promise;
  const replies = raw.trim().split("\n").map(JSON.parse);
  assert.equal(replies[0].result.protocolVersion, "2025-03-26");
  assert(
    replies[1].result.tools.some((t) => t.name === "xiaoxiong_plugins_list"),
  );
  assert(!replies[1].result.tools.some((t) => t.name.includes("secret_get")));
  assert.equal(c.calls[0][0], "draft.get");
  assert.equal(replies[2].result.isError, false);
});

test("CLI rejects surplus arguments before any application mutation", async () => {
  const client = fake();
  const result = await main(["task", "cancel", "task-1", "unexpected"], {
    clientFactory: () => client,
  });
  assert.equal(result.error.code, "INVALID_ARGUMENT");
  assert.equal(client.calls.length, 0);
});
