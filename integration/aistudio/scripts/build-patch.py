#!/usr/bin/env python3
"""Package exact upstream baselines and edited application sources, without applying."""
from pathlib import Path
import difflib, hashlib, json
ROOT = Path(__file__).resolve().parents[1]
BASE_COMMIT = '5423be21112b61b0728cf10a0bc9883e0e308581'
# Known upstream files must have exact baselines; never package an edited old
# file as a new file merely because an input snapshot was missing.
known = json.loads((ROOT / 'upstream-files.json').read_text())
assert known['baseCommit'] == BASE_COMMIT
for name, expected in known['existingFiles'].items():
    baseline = ROOT / 'baseline' / name
    if not baseline.is_file():
        raise SystemExit(f'Missing upstream baseline: {name}')
    raw = baseline.read_bytes()
    blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    if blob != expected:
        raise SystemExit(f'Upstream baseline hash mismatch: {name}')
entries, patch = [], []
for file in sorted((ROOT / 'overlay').rglob('*')):
    if not file.is_file():
        continue
    path = file.relative_to(ROOT / 'overlay').as_posix()
    old = ROOT / 'baseline' / path
    before, after = old.read_bytes() if old.exists() else b'', file.read_bytes()
    if old.exists() and before == after:
        continue
    entries.append({'path': path, 'beforeSha256': hashlib.sha256(before).hexdigest() if old.exists() else None,
                    'afterSha256': hashlib.sha256(after).hexdigest()})
    patch.append(f'diff --git a/{path} b/{path}\n')
    if not old.exists():
        patch.append('new file mode 100644\n')
    patch.extend(difflib.unified_diff(before.decode().splitlines(keepends=True), after.decode().splitlines(keepends=True),
                                    fromfile=f'a/{path}' if old.exists() else '/dev/null', tofile=f'b/{path}'))
(ROOT / 'aistudio-v2.patch').write_text(''.join(patch), encoding='utf-8')
(ROOT / 'manifest.json').write_text(json.dumps({'repository':'adam-web3-developer9/aistudio', 'baseCommit':BASE_COMMIT,
                                             'files':entries}, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'files':len(entries),'patch':'aistudio-v2.patch'}))
