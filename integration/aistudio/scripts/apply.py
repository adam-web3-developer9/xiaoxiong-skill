#!/usr/bin/env python3
"""Check/apply only the packaged source patch. Never builds, starts, pushes or deploys."""
from pathlib import Path
import argparse, hashlib, json, sys
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('checkout',type=Path)
p.add_argument('--apply',action='store_true',help='Write source files after every baseline passes (default: check only)')
a=p.parse_args(); checkout=a.checkout.resolve()
manifest=json.loads((ROOT/'manifest.json').read_text()); pending=[]; conflicts=[]
def digest(b):return hashlib.sha256(b).hexdigest()
for item in manifest['files']:
    target=checkout/item['path']; source=ROOT/'overlay'/item['path']
    if digest(source.read_bytes())!=item['afterSha256']:
        conflicts.append(item['path']+': packaged content differs from manifest');continue
    if target.is_symlink() or checkout not in target.resolve().parents:
        conflicts.append(item['path']+': target must stay inside checkout');continue
    current=digest(target.read_bytes()) if target.is_file() else None
    if current==item['afterSha256']:continue
    if current!=item['beforeSha256']:
        conflicts.append(item['path']+': source changed; review/rebase instead of overwriting');continue
    pending.append((target,source))
if conflicts:
    print(json.dumps({'ok':False,'conflicts':conflicts},ensure_ascii=False,indent=2));sys.exit(1)
if a.apply:
    for target,source in pending:
        target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(source.read_bytes())
print(json.dumps({'ok':True,'mode':'applied' if a.apply else 'check','files':len(pending),'baseCommit':manifest['baseCommit']}))
