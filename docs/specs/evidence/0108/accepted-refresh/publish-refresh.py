from pathlib import Path
from PIL import Image,ImageDraw
import json,hashlib
base=Path(__file__).resolve().parent
out=base/'publish-refresh';out.mkdir(exist_ok=True)
root=base/'evidence/accepted-refresh'
meta={'revision':(base/'candidate-new/REVISION').read_text().strip(),'binary_sha256':hashlib.sha256((base/'bin/nacre-shell').read_bytes()).hexdigest(),'geometry':[2880,1800],'scale':1.5,'pair_order':['QML reference','final Bead'],'sequence_interval_ms':25,'timing_note':'Screencopy proxy timestamps; not scanout.','sources':{}}
def load(path):
 im=Image.open(path).convert('RGB');assert im.size==(2880,1800)
 meta['sources'][str(path.relative_to(base/'evidence'))]={'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'size':list(im.size)}
 return im
for name in ['idle','bar-title-open','tabs-1']:
 box=(700,75,2180,175) if name.startswith('tabs') else (1000,0,1880,75)
 ims=[load(base/f'evidence/parity/qml/shots/{name}.png').crop(box),load(root/f'bead/shots/{name}.png').crop(box)]
 pair=Image.new('RGB',(ims[0].width*2,ims[0].height));pair.paste(ims[0],(0,0));pair.paste(ims[1],(ims[0].width,0));pair.save(out/f'{name}.png')
for name in ['bar-title-open-open','tabs-1-open']:
 box=(700,75,2180,175) if name.startswith('tabs') else (1000,0,1880,75)
 paths=[base/f'evidence/parity/qml/seq/{name}',root/f'bead/seq/{name}'];stamps=[json.loads((p/'stamps.json').read_text()) for p in paths]
 indices=list(range(0,min(map(len,stamps)),4));w=box[2]-box[0];h=box[3]-box[1]
 sheet=Image.new('RGB',(w*2,(h+22)*len(indices)),'#202020');draw=ImageDraw.Draw(sheet)
 for row,i in enumerate(indices):
  for column,(path,ts) in enumerate(zip(paths,stamps)):
   draw.text((column*w+4,row*(h+22)+3),f'{["QML","Bead"][column]} frame {i}: proxy {ts[i]} ms',fill='white')
   sheet.paste(load(path/f'{i:03}.jpg').crop(box),(column*w,row*(h+22)+22))
 sheet.save(out/f'series-{name}.png')
meta['title_retention']=json.loads((root/'bead/title-proof.json').read_text());assert meta['title_retention']['changed_title_states']==0
meta['state_errors']={name:value.get('errors',[]) for name,value in json.loads((root/'bead/drive.json').read_text()).items()};assert not any(meta['state_errors'].values())
guard=json.loads((base/'evidence/accepted-refresh-containment.json').read_text());meta['containment']={k:guard[k] for k in ['capture_returncode','child_membership_verified','effective_memory_max','effective_memory_swap_max','oom_delta']};assert guard['capture_returncode']==0 and guard['child_membership_verified'] and not any(guard['oom_delta'].values())
(out/'metadata.json').write_text(json.dumps(meta,indent=2)+'\n')
print(json.dumps({'revision':meta['revision'],'title':meta['title_retention'],'errors':meta['state_errors'],'containment':meta['containment']},indent=2))
