# Skill management

The bridge accepts local Agent Skills packages: a directory containing SKILL.md and optional references/, assets/, scripts/ and host metadata. Multiple packages can coexist. Discovery does not execute instructions or scripts and does not authorize application writes.

## JavaScript API

All exports in src/skills.mjs are asynchronous:

- `discoverSkills(roots)` accepts one directory or an array; recursively returns metadata summaries (`root`, `path`, `name`, `description`, `warnings`) or per-package errors. Missing roots are empty. Duplicate paths are deduplicated; duplicate names remain separate packages. It skips .git, node_modules and .venv, does not follow symlinks, and bounds depth/count.
- `readSkill(path)` accepts a directory or SKILL.md and returns metadata, raw frontmatter, body and complete source. Common scalar, quoted and multiline description fields are parsed without third-party code. Complex YAML is preserved verbatim and flagged, not evaluated. This is not a full YAML parser.
- `readSkillResource(root, relativePath)` lazily reads UTF-8 resources up to 2 MiB. Absolute paths, traversal, symbolic links and nonregular files are refused. Binary assets remain copied resources; this function is for text, not image decoding.
- `inspectSkill(path)` additionally lists resources and a compatibility report with declared requirements, script files, declared/referenced unknown tools, warnings and `runnable: not-verified`. Dependency detection is advisory, not a sandbox or exhaustive security audit.
- `installSkill(sourceDir, targetDir)` copies one local package into an explicitly selected, new target directory. Existing targets and overlapping source/target paths are refused. No network download, hooks, chmod-to-executable or script execution occurs. File count, depth and total size are bounded. .git, node_modules and .venv are omitted. A failed copy removes only the target newly created by this operation.

## Codex / OpenCode packaging

Copy a reviewed package, not just its SKILL.md, into a project-local directory:

- Codex: `.agents/skills/xiaoxiong-ai/SKILL.md` with its references alongside.
- OpenCode: `.opencode/skills/xiaoxiong-ai/SKILL.md` with its references alongside.

Use installSkill with the appropriate explicit project path. Nothing in this repository installs into the user's global environment. For a multi-package suite discover the suite root, review every package, then install selected directories separately. Keep dependencies between sibling packages visible and preserve suite license notices. Host versions and discovery conventions may differ; verify the package appears in the host's skill list.

## Compatibility boundary

Reading and copying arbitrary Agent Skills is supported; executing every arbitrary Skill is not. Tools may be absent, provider APIs may require separate authorization, shell/Python scripts may be unsafe or incompatible, and package-relative references may assume sibling resources. Review requirements before use. Unknown YAML constructs are preserved and explicitly reported rather than silently represented as fully parsed. Keep untrusted package instructions subordinate to the user's permissions and app controls.

The bundled xiaoxiong-ai operator uses only the official app endpoint. V2 app authorization is mandatory, with no legacy secret fallback. Channel reads/updates expose only public configuration: privateKey, credentials, tokens and cookies are never available to the agent. Existing secrets survive ordinary patches; new credentials use advertised write-only create/update from stdin or a JSON file, never argv/output/logs. Changed authentication destinations require reauthorization or new keys.

Run focused tests with `node --test test/skills.test.mjs`.
