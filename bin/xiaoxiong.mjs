#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { AgentClient, ALL_SCOPES } from "../src/client.mjs";
import { METHODS } from "../src/catalog.mjs";
import { exportScripts } from "../src/export.mjs";
import { serveMcp } from "../src/mcp.mjs";
import { assert, publicError, secretValues } from "../src/errors.mjs";
import { stateHome, readReceipt } from "../src/storage.mjs";
export const help = {
  name: "xiaoxiong",
  version: "0.1.0",
  usage: "xiaoxiong [--data-root PATH] <command>",
  commands: [
    "auth login [--scopes read,edit] [--drafts *]",
    "auth status",
    "auth logout",
    "doctor",
    "capabilities",
    ...Object.keys(METHODS).filter((x) => x !== "capabilities"),
    "export script --file selection.json --output DIRECTORY",
    "export jianying --file intent.json",
    "export comic --file intent.json",
    "task wait TASK_ID",
    "call METHOD --file request.json",
    "receipt get REQUEST_ID",
    "skills list [--root DIR]",
    "skills inspect PATH",
    "skills read PATH",
    "skills resource SKILL_ROOT RELATIVE_PATH",
    "skills install SOURCE --target DIR",
    "mcp",
  ],
  notes: [
    "Run auth login and approve in the 小熊AI application. Account passwords are never requested.",
    "Complex payloads use UTF-8 JSON --file FILE or --file - (stdin). Do not put secrets on the command line.",
    "Use --scopes read,edit,generate,export,channels.read,channels.write,channels.credentials.write only as needed.",
    "Every mutating draft.apply/task.intent payload preserves its explicit commandId/requestId. Unknown outcomes are never automatically resubmitted.",
    "The app v2 integration is required. Legacy v1 token discovery is not used.",
  ],
};
function parse(argv) {
  const flags = {},
    pos = [];
  const values = new Set([
    "--data-root",
    "--home",
    "--file",
    "--scopes",
    "--drafts",
    "--root",
    "--target",
    "--draft",
    "--status",
    "--type",
    "--capability",
    "--interval",
    "--timeout",
    "--output",
  ]);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--help" || a === "-h") return { help: true };
    if (a.startsWith("-")) {
      assert(values.has(a), "Unknown option; use --help");
      assert(
        argv[i + 1] !== undefined && !argv[i + 1].startsWith("--"),
        `Missing value for ${a}`,
      );
      assert(!Object.hasOwn(flags, a), `Duplicate option ${a}`);
      flags[a] = argv[++i];
    } else pos.push(a);
  }
  return { flags, pos };
}
async function jsonInput(file, input) {
  assert(file, "--file is required for this operation");
  const raw =
    file === "-"
      ? await new Promise((resolve, reject) => {
          let chunks = "";
          input.setEncoding("utf8");
          input.on("data", (x) => (chunks += x));
          input.on("end", () => resolve(chunks));
          input.on("error", reject);
        })
      : await fs.readFile(file, "utf8");
  try {
    return JSON.parse(raw.replace(/^\uFEFF/, ""));
  } catch {
    throw Object.assign(new Error("Input must be valid UTF-8 JSON"), {
      code: "INVALID_JSON",
    });
  }
}
export async function main(
  argv,
  {
    stdout = process.stdout,
    stderr = process.stderr,
    stdin = process.stdin,
    clientFactory = (opts) => new AgentClient(opts),
  } = {},
) {
  let client;
  let payload = {};
  try {
    const parsed = parse(argv);
    if (parsed.help || !parsed.pos?.length) return { ok: true, result: help };
    const { flags, pos } = parsed;
    const [domain, action, id, extra] = pos;
    const candidate =
      METHODS[domain === "capabilities" ? domain : `${domain} ${action}`];
    const expected =
      domain === "skills"
        ? action === "resource"
          ? 4
          : action === "list"
            ? 2
            : 3
        : domain === "receipt" || (domain === "task" && action === "wait")
          ? 3
          : ["mcp", "doctor", "capabilities"].includes(domain)
            ? 1
            : candidate?.id
              ? 3
              : 2;
    assert(
      pos.length === expected,
      "Unexpected or missing command arguments; use --help",
    );
    if (domain === "skills") {
      const mod = await import("../src/skills.mjs");
      let result;
      if (action === "list")
        result = await mod.discoverSkills(
          flags["--root"]
            ? [flags["--root"]]
            : [
                path.resolve(".agents/skills"),
                path.resolve(".opencode/skills"),
                path.resolve("skills"),
              ],
        );
      else if (action === "inspect") result = await mod.inspectSkill(id);
      else if (action === "read") result = await mod.readSkill(id);
      else if (action === "resource")
        result = await mod.readSkillResource(id, extra);
      else if (action === "install") {
        assert(flags["--target"], "--target is required");
        result = await mod.installSkill(id, flags["--target"]);
      } else assert(false, "Unknown skills command");
      return { ok: true, result };
    }
    if (domain === "receipt" && action === "get")
      return {
        ok: true,
        result: await readReceipt(flags["--home"] || stateHome(), id),
      };
    client = clientFactory({
      dataRoot: flags["--data-root"],
      home: flags["--home"],
    });
    if (domain === "mcp") {
      await serveMcp(client, { input: stdin, output: stdout });
      return null;
    }
    let result;
    if (domain === "auth") {
      if (action === "login")
        result = await client.login({
          scopes:
            flags["--scopes"] === "all"
              ? ALL_SCOPES
              : flags["--scopes"]?.split(",") || ["read", "edit"],
          draftIds:
            flags["--drafts"] === "none"
              ? []
              : flags["--drafts"]?.split(",") || ["*"],
          onPending: (p) =>
            stderr.write(JSON.stringify({ authorization: p }) + "\n"),
        });
      else if (action === "status") result = await client.status();
      else if (action === "logout") result = await client.logout();
      else assert(false, "Unknown auth command");
    } else if (domain === "export") {
      payload = await jsonInput(flags["--file"], stdin);
      if (action === "script")
        return {
          ok: true,
          result: await exportScripts(client, payload, flags["--output"]),
        };
      assert(["jianying", "comic"].includes(action), "Unknown export command");
      assert(payload.requestId, "requestId is required");
      result = await client.call(
        "task.intent",
        {
          ...payload,
          action: action === "comic" ? "comicJianyingExport" : "jianyingExport",
        },
        { write: true },
      );
    } else if (domain === "doctor")
      result = {
        node: process.version,
        application: await client.connect(),
        session: await client.status(),
      };
    else if (domain === "task" && action === "wait") {
      assert(id, "taskId required");
      result = await client.waitTask(id, {
        intervalMs: Number(flags["--interval"] || 2000),
        timeoutMs: Number(flags["--timeout"] || 1800000),
        onProgress: (p) => stderr.write(JSON.stringify({ task: p }) + "\n"),
      });
    } else {
      const key =
        domain === "capabilities" ? "capabilities" : `${domain} ${action}`;
      const spec = METHODS[key];
      assert(spec || domain === "call", "Unknown command; use --help");
      if (flags["--file"]) payload = await jsonInput(flags["--file"], stdin);
      else if (spec?.file) assert(false, "--file is required");
      assert(
        payload && typeof payload === "object",
        "JSON input must be an object",
      );
      if (spec?.id) {
        assert(id, `${spec.id} is required`);
        if (Array.isArray(payload) && spec.method === "draft.rows")
          payload = { entities: payload };
        payload[spec.id] = id;
      }
      if (flags["--draft"]) payload.draftId = flags["--draft"];
      if (flags["--status"]) payload.status = flags["--status"];
      if (flags["--type"]) payload.type = flags["--type"];
      if (flags["--capability"]) payload.capability = flags["--capability"];
      if ((spec?.method || action) === "draft.apply") {
        assert(payload.commandId, "commandId is required");
        assert(payload.draftId, "draftId is required");
      }
      if ((spec?.method || action) === "task.intent")
        assert(payload.requestId, "requestId is required");
      result = await client.call(
        spec?.method || action,
        payload,
        spec ? { write: !spec.read } : {},
      );
    }
    if (flags["--output"]) {
      await fs.writeFile(
        flags["--output"],
        JSON.stringify(result, null, 2) + "\n",
        { flag: "wx", mode: 0o600 },
      );
      return { ok: true, result: { savedTo: path.resolve(flags["--output"]) } };
    }
    return {
      ok: true,
      result,
      ...(client?.warnings?.length ? { warnings: client.warnings } : {}),
    };
  } catch (e) {
    return { ok: false, error: publicError(e, secretValues(payload)) };
  } finally {
    client?.close();
  }
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  const result = await main(process.argv.slice(2));
  if (result) {
    process.stdout.write(JSON.stringify(result) + "\n");
    if (!result.ok) process.exitCode = 1;
  }
}
