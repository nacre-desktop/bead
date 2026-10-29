#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Capability/lifetime proof reusing the actual nested bar+sound-panel workload."""
import collections,importlib.util,json,os,pathlib,re
ROOT=pathlib.Path(__file__).resolve().parent.parent

def sample(env,raw,compositor,stop):
    os.environ['BEAD_BACKGROUND_BLUR']='1'
    spec=importlib.util.spec_from_file_location('panel_proof',ROOT/'spikes/panel-proof.py')
    panel=importlib.util.module_from_spec(spec);spec.loader.exec_module(panel)
    panel.sample(env,raw,compositor,stop)
    text=(raw/'panel.log').read_text()
    header=re.search(r'BACKGROUND_EFFECT requested=true advertised=(true|false)',text)
    assert header,'missing capability-discovery marker'
    advertised=header[1]=='true'
    caps=[int(n) for n in re.findall(r'BACKGROUND_EFFECT advertised=true capabilities=(\d+)',text)]
    created=re.findall(r'BACKGROUND_EFFECT created=(\d+)',text)
    destroyed=re.findall(r'BACKGROUND_EFFECT destroyed=(\d+)',text)
    enabled='BACKGROUND_EFFECT enabled=true' in text
    if not advertised or not any(c&1 for c in caps):assert not created and not enabled,'effect requested without advertised blur'
    else:assert created and enabled,'advertised blur did not create an effect'
    assert collections.Counter(created)==collections.Counter(destroyed),'background effect outlived its surface'
    assert 'protocol_error' not in text and ' code=unavailable' not in text
    result={'compositor':compositor,'advertised':advertised,'capabilities':caps,'created':len(created),'destroyed':len(destroyed),'enabled':enabled,'all_owned_surfaces_released':True,'visual_parity':'not accepted; protocol/lifetime proof'}
    (raw/'background-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('BACKGROUND_PROOF '+json.dumps(result),flush=True)
