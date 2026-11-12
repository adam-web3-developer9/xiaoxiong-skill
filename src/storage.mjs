import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { randomUUID, createHash } from "node:crypto";
import { failure } from "./errors.mjs";
export function stateHome() {
  return (
    process.env.XIAOXIONG_AGENT_HOME ||
    path.join(
      process.env.XDG_STATE_HOME || path.join(os.homedir(), ".local", "state"),
      "xiaoxiong-agent",
    )
  );
}
export async function readJSON(file, fallback) {
  try {
    return JSON.parse((await fs.readFile(file, "utf8")).replace(/^\uFEFF/, ""));
  } catch (e) {
    if (e.code === "ENOENT") return fallback;
    throw failure("STATE_INVALID", "Stored agent state cannot be read");
  }
}
export async function atomicJSON(file, data) {
  await fs.mkdir(path.dirname(file), { recursive: true, mode: 0o700 });
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(temp, JSON.stringify(data, null, 2) + "\n", {
      flag: "wx",
      mode: 0o600,
    });
    await fs.rename(temp, file);
    await fs.chmod(file, 0o600);
  } finally {
    await fs.rm(temp, { force: true }).catch(() => {});
  }
}
export function profileKey(dataRoot) {
  return createHash("sha256")
    .update(path.resolve(dataRoot))
    .digest("hex")
    .slice(0, 24);
}
export function credentialPath(home, dataRoot) {
  return path.join(home, "sessions", `${profileKey(dataRoot)}.json`);
}
export async function loadCredential(home, dataRoot) {
  return readJSON(credentialPath(home, dataRoot), null);
}
export async function saveCredential(home, dataRoot, credential) {
  await atomicJSON(credentialPath(home, dataRoot), credential);
}
export async function deleteCredential(home, dataRoot) {
  await fs.rm(credentialPath(home, dataRoot), { force: true });
}
export async function writeReceipt(home, requestId, record) {
  // Receipt journals contain identity/hash/outcome, never raw prompts or channel credentials.
  const name = createHash("sha256").update(requestId).digest("hex");
  await atomicJSON(path.join(home, "receipts", `${name}.json`), record);
}
export async function readReceipt(home, requestId) {
  return readJSON(
    path.join(
      home,
      "receipts",
      `${createHash("sha256").update(requestId).digest("hex")}.json`,
    ),
    null,
  );
}
