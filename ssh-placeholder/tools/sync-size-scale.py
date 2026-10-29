#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Synchronize factual scale data from the owner's selected Nacre revision."""
import argparse,json,pathlib,sys
p=argparse.ArgumentParser();p.add_argument('source',type=pathlib.Path);p.add_argument('--check',action='store_true');a=p.parse_args()
destination=pathlib.Path(__file__).resolve().parent.parent/'crates/bead-ui/assets/size-scale.json'
data=json.loads(a.source.read_bytes())
for group in ['spacing','radii','text','icons']:
    if not isinstance(data.get(group),dict) or any(not isinstance(v,(int,float)) or not 0<v<=10000 for v in data[group].values()):
        raise SystemExit('invalid scale group '+group)
if a.check:
    if json.loads(destination.read_bytes())!=data:raise SystemExit('shared size scale differs from source')
    print('Shared size scale matches source')
else:destination.write_bytes(a.source.read_bytes())
