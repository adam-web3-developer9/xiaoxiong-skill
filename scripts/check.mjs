import fs from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
let count = 0;
async function walk(dir) {
  for (const e of await fs.readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) await walk(p);
    else if (p.endsWith(".mjs")) {
      const r = spawnSync(process.execPath, ["--check", p], {
        stdio: "inherit",
      });
      if (r.status) process.exit(r.status);
      count++;
    }
  }
}
for (const dir of ["bin", "src", "scripts", "test", "examples"])
  await walk(dir);
console.log(`Syntax checked ${count} modules`);
