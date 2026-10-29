#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Analyze actual views against Rust-generated SDK declarations in a private mirror."""
import argparse,json,os,pathlib,re,shutil,signal,subprocess,sys,tempfile
ROOT=pathlib.Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--analyzer',required=True);p.add_argument('--binary',required=True);p.add_argument('--check-generated',action='store_true');a=p.parse_args()
with tempfile.TemporaryDirectory(prefix='bead-luau-') as directory:
    base=pathlib.Path(directory);bundle=base/'shell';shutil.copytree(ROOT/'shell',bundle)
    types=bundle/'types';types.mkdir(exist_ok=True)
    subprocess.run([a.binary,'--generate-types',str(types/'bead_ui.luau')],check=True,cwd=ROOT)
    generated=types/'bead_ui.luau';tracked=ROOT/'shell/types/bead_ui.luau'
    if a.check_generated:
        if not tracked.exists() or tracked.read_bytes()!=generated.read_bytes():raise SystemExit('Rust/Luau SDK declarations differ; regenerate')
    else:shutil.copyfile(generated,tracked)
    shutil.copyfile(ROOT/'crates/bead-ui/assets/kit.luau',types/'kit_impl.luau')
    tokens=json.loads((ROOT/'crates/bead-ui/assets/size-scale.json').read_text())
    tokens['gradients']=True  # the host adds it when it builds the kit (the interfaceGradients setting)
    def literal(v):
        if isinstance(v,dict):return '{'+','.join('['+json.dumps(k)+']='+literal(x) for k,x in v.items())+'}'
        return json.dumps(v)
    (types/'bead_kit.luau').write_text('local ui=require("./bead_ui");local factory=require("./kit_impl");return factory(ui,'+literal(tokens)+')\n')
    # The host generates plugins/index.luau per bundle (spec 0203); analyze against an empty index.
    (bundle/'plugins').mkdir(exist_ok=True);(bundle/'plugins/index.luau').write_text('return {api=1,user_dir=false,notice="",plugins={}}\n')
    files=list(bundle.rglob('*.luau'))
    for f in files:
        text=f.read_text()
        for name,target in [('bead.ui','bead_ui'),('bead.kit','bead_kit')]:
            relative=os.path.relpath(types/target,f.parent)
            if not relative.startswith('.'):relative='./'+relative
            text=text.replace('require("'+name+'")','require('+json.dumps(relative)+')')
        # Behaviour scripts get the global tr(id,args) from the host, not from a module.
        if re.search(r'(?<![.\w])tr\(',text):text='local tr:(string,any?)->string=(nil::any);'+text
        if re.search(r'(?<![.\w])copy_value\(',text):text='local copy_value:(any)->any=(nil::any);'+text
        f.write_text(text)
    result=subprocess.run([a.analyzer,'--mode=nonstrict',*map(str,files)],cwd=base,text=True,capture_output=True)
    print(result.stdout,end='');print(result.stderr,end='')
    if result.returncode:
        if result.returncode < 0:
            try:failure=signal.Signals(-result.returncode).name
            except ValueError:failure=f'SIGNAL_{-result.returncode}'
            print(f'LUAU_ANALYZE_FAILED signal={failure}',file=sys.stderr)
            raise SystemExit(1)
        raise SystemExit(result.returncode)
    print('LUAU_ANALYZE_OK',len(files),'modules; actual source mirror, generated Rust SDK')
