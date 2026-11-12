import * as fs from "node:fs/promises";
import path from "node:path";
import { constants } from "node:fs";

const MAX_TEXT = 2 * 1024 * 1024;
const SKIP = new Set([".git", "node_modules", ".venv"]);
const inside = (root, file) => {
  const rel = path.relative(root, file);
  return (
    rel === "" ||
    (!rel.startsWith(`..${path.sep}`) && rel !== ".." && !path.isAbsolute(rel))
  );
};
async function plain(file) {
  const info = await fs.lstat(file);
  if (info.isSymbolicLink())
    throw new Error(`Symbolic links are not supported: ${file}`);
  return info;
}
async function safeResource(root, relative) {
  if (
    typeof relative !== "string" ||
    !relative ||
    relative.includes("\\") ||
    relative.includes("\0") ||
    path.isAbsolute(relative) ||
    /^[A-Za-z]:/.test(relative)
  )
    throw new Error("Expected a package-relative resource path");
  root = path.resolve(root);
  await plain(root);
  const file = path.resolve(root, relative);
  if (!inside(root, file)) throw new Error("Resource escapes skill package");
  let current = root;
  for (const part of path
    .relative(root, file)
    .split(path.sep)
    .filter(Boolean)) {
    current = path.join(current, part);
    await plain(current);
  }
  if (!inside(await fs.realpath(root), await fs.realpath(file)))
    throw new Error("Resource escapes skill package");
  return file;
}
async function textFile(file) {
  const handle = await fs.open(
    file,
    constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0),
  );
  try {
    const s = await handle.stat();
    if (!s.isFile() || s.size > MAX_TEXT)
      throw new Error(
        "Resource is not a text-sized regular file (2 MiB limit)",
      );
    return await handle.readFile("utf8");
  } finally {
    await handle.close();
  }
}
function scalar(raw) {
  raw = raw.trim();
  if (raw.startsWith('"') && raw.endsWith('"')) {
    try {
      return JSON.parse(raw);
    } catch {}
  }
  if (raw.startsWith("'") && raw.endsWith("'"))
    return raw.slice(1, -1).replace(/''/g, "'");
  return raw.replace(/\s+#.*$/, "").trim();
}
// Intentionally not a YAML evaluator. Preserve the full source and flag complex metadata.
function frontmatter(source) {
  const match = source
    .replace(/^\uFEFF/, "")
    .match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)([\s\S]*)$/);
  if (!match)
    return {
      metadata: {},
      frontmatter: "",
      body: source,
      warnings: ["Missing or unterminated YAML frontmatter"],
    };
  const metadata = Object.create(null),
    warnings = [],
    lines = match[1].split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^([A-Za-z][\w-]*):\s*(.*)$/);
    if (!m) {
      if (lines[i].trim() && !lines[i].trim().startsWith("#"))
        warnings.push(
          "Complex YAML metadata preserved as raw frontmatter; inspect it in the host",
        );
      continue;
    }
    let value = m[2];
    if (/^[>|][-+]?$/.test(value)) {
      const parts = [];
      while (
        i + 1 < lines.length &&
        (/^\s/.test(lines[i + 1]) || !lines[i + 1])
      )
        parts.push(lines[++i].replace(/^\s+/, ""));
      value = parts.join(value[0] === ">" ? " " : "\n").trim();
    } else value = scalar(value);
    if (Object.hasOwn(metadata, m[1]))
      warnings.push(`Duplicate metadata key: ${m[1]}`);
    metadata[m[1]] = value;
  }
  return {
    metadata,
    frontmatter: match[1],
    body: match[2],
    warnings: [...new Set(warnings)],
  };
}
export async function readSkill(input) {
  const absolute = path.resolve(input),
    info = await plain(absolute);
  const root = info.isDirectory() ? absolute : path.dirname(absolute);
  if (!info.isDirectory() && path.basename(absolute) !== "SKILL.md")
    throw new Error("Expected a skill directory or SKILL.md");
  const source = await readSkillResource(root, "SKILL.md");
  const parsed = frontmatter(source);
  const name = parsed.metadata.name || path.basename(root),
    description = parsed.metadata.description || "";
  if (!parsed.metadata.name) parsed.warnings.push("Missing name");
  if (!description) parsed.warnings.push("Missing description");
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name) || name.length > 64)
    parsed.warnings.push("Name does not follow Agent Skills naming convention");
  return {
    root,
    path: path.join(root, "SKILL.md"),
    name,
    description,
    ...parsed,
    source,
  };
}
export async function readSkillResource(root, relative) {
  return textFile(await safeResource(root, relative));
}
export async function discoverSkills(roots) {
  if (!Array.isArray(roots)) roots = [roots];
  const results = [],
    visited = new Set();
  let count = 0;
  async function visit(dir, depth = 0) {
    if (++count > 10000 || depth > 20)
      throw new Error("Skill discovery traversal limit exceeded");
    dir = path.resolve(dir);
    if (visited.has(dir)) return;
    visited.add(dir);
    let info;
    try {
      info = await plain(dir);
    } catch (e) {
      if (e.code === "ENOENT") return;
      results.push({ root: dir, error: e.message });
      return;
    }
    if (!info.isDirectory()) return;
    const entries = await fs.readdir(dir, { withFileTypes: true });
    if (entries.some((e) => e.name === "SKILL.md")) {
      try {
        const {
          name,
          description,
          warnings,
          path: file,
        } = await readSkill(dir);
        results.push({ root: dir, path: file, name, description, warnings });
      } catch (e) {
        results.push({ root: dir, error: e.message });
      }
    }
    for (const e of entries.sort((a, b) => a.name.localeCompare(b.name)))
      if (e.isDirectory() && !SKIP.has(e.name))
        await visit(path.join(dir, e.name), depth + 1);
  }
  for (const root of roots) await visit(root);
  return results;
}
async function inventory(root) {
  const files = [];
  let bytes = 0;
  async function walk(dir, depth = 0) {
    if (depth > 20) throw new Error("Package depth limit exceeded");
    for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name),
        s = await plain(file);
      if (s.isDirectory()) {
        if (SKIP.has(entry.name)) continue;
        await walk(file, depth + 1);
      } else if (s.isFile()) {
        bytes += s.size;
        files.push(path.relative(root, file).split(path.sep).join("/"));
        if (files.length > 10000 || bytes > 100 * 1024 * 1024)
          throw new Error("Package exceeds 10,000 files or 100 MiB");
      } else
        throw new Error("Only regular files and directories are supported");
    }
  }
  await walk(root);
  return files.sort();
}
export async function inspectSkill(input) {
  const skill = await readSkill(input),
    resources = await inventory(skill.root);
  const externalScripts = resources.filter(
    (f) =>
      f.startsWith("scripts/") ||
      /\.(?:py|sh|bash|ps1|exe|mjs|cjs|js|bat)$/i.test(f),
  );
  const declaredTools = String(
    skill.metadata["allowed-tools"] || skill.metadata.tools || "",
  )
    .replace(/[\[\]"']/g, "")
    .split(/[,\s]+/)
    .filter(Boolean);
  const toolReferences = [
    ...skill.source.matchAll(/\b(?:mcp__[\w-]+|[\w-]+\.[\w-]+)\b/g),
  ]
    .map((m) => m[0])
    .filter((s) => !/\.(md|py|sh|json|yaml|png|jpg|com|org)$/i.test(s));
  const requires = {
    declared: skill.metadata.requires || skill.metadata.compatibility || null,
    tools: declaredTools,
    scripts: externalScripts,
  };
  return {
    ...skill,
    resources,
    compatibility: {
      format:
        skill.description && skill.metadata.name
          ? "agent-skills-readable"
          : "needs-metadata-review",
      runnable: "not-verified",
      requires,
      externalScripts,
      unknownTools: [...new Set([...declaredTools, ...toolReferences])],
      warnings: [
        ...skill.warnings,
        ...(externalScripts.length
          ? [
              "Scripts are copied only; runtime dependencies, safety and execution are not verified",
            ]
          : []),
        "Host tool availability, permissions and semantic compatibility require review; discovery is not execution approval",
      ],
    },
  };
}
export async function installSkill(sourceDir, targetDir) {
  const source = path.resolve(sourceDir),
    target = path.resolve(targetDir);
  if (inside(source, target) || inside(target, source))
    throw new Error("Source and target must not overlap");
  await readSkill(source);
  const resources = await inventory(source);
  await fs.mkdir(path.dirname(target), { recursive: true });
  // Exclusive directory creation: never overwrite an existing user package.
  await fs.mkdir(target);
  try {
    for (const relative of resources) {
      const input = await safeResource(source, relative),
        output = path.join(target, relative);
      await fs.mkdir(path.dirname(output), { recursive: true });
      const handle = await fs.open(
        input,
        constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0),
      );
      try {
        if (!(await handle.stat()).isFile())
          throw new Error("Source changed during install");
        await fs.writeFile(output, await handle.readFile(), {
          flag: "wx",
          mode: 0o600,
        });
      } finally {
        await handle.close();
      }
    }
    return {
      ...(await inspectSkill(target)),
      installed: true,
      executedScripts: false,
    };
  } catch (e) {
    await fs.rm(target, { recursive: true, force: true });
    throw e;
  }
}
