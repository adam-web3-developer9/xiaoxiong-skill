import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { failure } from "./errors.mjs";
export function defaultDataRoots(
  env = process.env,
  platform = process.platform,
  home = os.homedir(),
) {
  const appData =
    platform === "win32"
      ? env.APPDATA || path.join(home, "AppData", "Roaming")
      : platform === "darwin"
        ? path.join(home, "Library", "Application Support")
        : env.XDG_CONFIG_HOME || path.join(home, ".config");
  return [
    ...new Set(
      [
        env.AISTUDIO_DATA_ROOT,
        env.AISTUDIO_USER_DATA &&
          path.join(env.AISTUDIO_USER_DATA, "aistudio-data"),
        path.join(appData, "aistudio", "aistudio-data"),
        env.LOCALAPPDATA &&
          path.join(env.LOCALAPPDATA, "aistudio", "aistudio-data"),
      ]
        .filter(Boolean)
        .map((v) => path.resolve(v)),
    ),
  ];
}
function normalizedRoot(value) {
  const resolved = path.resolve(value);
  return process.platform === "win32" ? resolved.toLowerCase() : resolved;
}
function localEndpoint(endpoint) {
  if (typeof endpoint !== "string" || !endpoint) return false;
  return process.platform === "win32"
    ? endpoint.startsWith("\\\\.\\pipe\\")
    : path.isAbsolute(endpoint);
}
async function runningMarkers(directory) {
  let files;
  try {
    files = await fs.readdir(directory, { withFileTypes: true });
  } catch {
    return [];
  }
  const markers = [];
  for (const file of files) {
    if (
      !file.isFile() ||
      !/^aistudio-agent-v2-\d+-[0-9a-f-]+\.json$/i.test(file.name)
    )
      continue;
    try {
      const filename = path.join(directory, file.name);
      const stat = await fs.lstat(filename);
      if (!stat.isFile() || (process.getuid && stat.uid !== process.getuid()))
        continue;
      const marker = JSON.parse(await fs.readFile(filename, "utf8"));
      if (
        marker.protocol !== 2 ||
        typeof marker.dataRoot !== "string" ||
        !path.isAbsolute(marker.dataRoot) ||
        !localEndpoint(marker.endpoint) ||
        !Number.isSafeInteger(marker.pid) ||
        marker.pid <= 0 ||
        typeof marker.instanceId !== "string"
      )
        continue;
      // Signal 0 only checks whether this process exists. Never signal or stop it.
      process.kill(marker.pid, 0);
      markers.push(marker);
    } catch {
      /* Ignore stale or unreadable public discovery markers. */
    }
  }
  return markers;
}
export async function discover({
  dataRoot,
  roots = defaultDataRoots(),
  discoveryDirectory = os.tmpdir(),
} = {}) {
  const matches = [];
  const markers = dataRoot ? [] : await runningMarkers(discoveryDirectory);
  const candidates = new Map(
    (dataRoot ? [dataRoot] : roots).map((root) => [
      normalizedRoot(root),
      { root: path.resolve(root) },
    ]),
  );
  for (const marker of markers)
    candidates.set(normalizedRoot(marker.dataRoot), {
      root: marker.dataRoot,
      marker,
    });
  let legacy = false;
  for (const { root, marker } of candidates.values()) {
    let descriptor;
    try {
      descriptor = JSON.parse(
        await fs.readFile(path.join(root, "agent-v2-connection.json"), "utf8"),
      );
    } catch (error) {
      if (error.code === "ENOENT") {
        try {
          await fs.access(path.join(root, "agent-data-connection.json"));
          legacy = true;
        } catch {}
        continue;
      }
      if (marker && !dataRoot) continue;
      throw failure(
        "DISCOVERY_INVALID",
        "Application discovery descriptor is invalid",
      );
    }
    if (
      !localEndpoint(descriptor.endpoint) ||
      (!descriptor.protocols?.includes(2) && descriptor.protocol !== 2) ||
      normalizedRoot(descriptor.dataRoot || root) !== normalizedRoot(root)
    ) {
      if (marker && !dataRoot) continue;
      throw failure(
        "DISCOVERY_INVALID",
        "Application discovery does not match protocol or selected data directory",
      );
    }
    if (
      marker &&
      (marker.instanceId !== descriptor.instanceId ||
        marker.pid !== descriptor.pid ||
        marker.endpoint !== descriptor.endpoint)
    )
      continue;
    matches.push({ ...descriptor, dataRoot: path.resolve(root) });
  }
  if (matches.length > 1)
    throw failure(
      "MULTIPLE_APPLICATIONS",
      "Multiple application profiles found. Select --data-root.",
      { profiles: matches.map((d) => d.dataRoot) },
    );
  if (!matches.length)
    throw failure(
      legacy ? "APP_UPGRADE_REQUIRED" : "APP_NOT_FOUND",
      legacy
        ? "This application exposes only the legacy developer bridge. Apply the v2 integration and restart the application."
        : "Start 小熊AI, or specify its data directory with --data-root.",
    );
  return matches[0];
}
