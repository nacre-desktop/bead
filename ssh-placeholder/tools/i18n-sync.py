#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Keep shell/i18n/{en,context}.json equal to the bead.* entries of a Nacre checkout.

Translators work in Nacre's nacre/shell/i18n; Bead embeds the same English text so a
daemon-less preview reads correctly. `--check` fails on drift, `--write` copies Bead's
entries into Nacre (the Bead tables are the authoring source for bead.* ids)."""
import argparse,json,pathlib,sys
ROOT=pathlib.Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('nacre',type=pathlib.Path);p.add_argument('--write',action='store_true');a=p.parse_args()
for name in ('en.json','context.json'):
    own=json.loads((ROOT/'shell/i18n'/name).read_text())
    path=a.nacre/'nacre/shell/i18n'/name
    theirs=json.loads(path.read_text())
    mirrored={k:v for k,v in theirs.items() if k.startswith('bead.')}
    if mirrored==own:continue
    if not a.write:sys.exit(f'{name}: bead.* entries differ from {path}')
    # Nacre's file is grouped, not sorted: keep its order and insert the bead.* block
    # before the first key that sorts after it.
    rest=[(k,v) for k,v in theirs.items() if not k.startswith('bead.')]
    at=next((i for i,(k,_) in enumerate(rest) if k>'bead.'),len(rest))
    merged=dict(rest[:at]+sorted(own.items())+rest[at:])
    path.write_text(json.dumps(merged,indent=2,ensure_ascii=False)+'\n')
    print('wrote',path)
print('bead.* tables in sync')
