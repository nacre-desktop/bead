"""Crop private native captures; publish no page body or machine details."""
from pathlib import Path
import hashlib,json
from PIL import Image,ImageDraw
base=Path(__file__).resolve().parent
source=base/'evidence/parity'
out=base/'publish';out.mkdir(exist_ok=True)
meta={'geometry':[2880,1800],'scale':1.5,'pair_order':['QML','Bead'],'sequence_interval_ms':25,'timing_note':'Screencopy launch/completion proxy timestamps; not physical presentation timing.','sources':{},'series':{}}
def read(p):
    im=Image.open(p).convert('RGB')
    assert im.size==(2880,1800),(p,im.size)
    meta['sources'][str(p.relative_to(source))]={'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'size':list(im.size)}
    return im
for name in ['idle','bar-title-hover','bar-title-open','tabs-0','tabs-1','tabs-2','tabs-3','tabs-hover','tabs-pressed']:
    box=(1000,0,1880,75) if not name.startswith('tabs') else (700,75,2180,175)
    ims=[read(source/arm/'shots'/f'{name}.png').crop(box) for arm in ['qml','bead']]
    pair=Image.new('RGB',(ims[0].width*2,ims[0].height))
    pair.paste(ims[0],(0,0));pair.paste(ims[1],(ims[0].width,0));pair.save(out/f'{name}.png')
for name in ['bar-title-open-open','tabs-0-open','tabs-1-open','tabs-2-open','tabs-3-open']:
    box=(1000,0,1880,75) if name.startswith('bar') else (700,75,2180,175)
    arms={}
    for arm in ['qml','bead']:
        directory=source/arm/'seq'/name
        stamps=json.loads((directory/'stamps.json').read_text())
        rows=[];images=[]
        for stamp,p in zip(stamps,sorted(directory.glob('*.jpg'))):
            im=read(p);images.append(im.crop(box))
            pixels=im.load();lit=[]
            if name.startswith('tabs'):
                for x in range(700,2180):
                    weight=sum(1 for y in range(141,156) if (c:=pixels[x,y])[1]>100 and c[1]>c[0]+20 and c[2]>c[0]+10)
                    if weight:lit.append((x,weight))
            center=round(sum(x*w for x,w in lit)/sum(w for _,w in lit),2) if lit else None
            rows.append({'capture_proxy_ms':stamp,'pearl_x':center})
        arms[arm]=(images,rows)
        meta['series'][arm+'/'+name]=rows
    # Even samples plus every changed pearl position demonstrate the retained strip.
    rows=arms['bead'][1]
    changed=[i for i in range(1,len(rows)) if rows[i]['pearl_x']!=rows[i-1]['pearl_x']]
    indices=sorted(set([0,len(rows)-1,*range(0,len(rows),max(1,len(rows)//8)),*changed]))
    if len(indices)>18:indices=[indices[round(i*(len(indices)-1)/17)] for i in range(18)]
    box=(1000,0,1880,75) if name.startswith('bar') else (700,75,2180,175)
    w=box[2]-box[0];h=box[3]-box[1]
    sheet=Image.new('RGB',(w*2,(h+22)*len(indices)), '#202020');draw=ImageDraw.Draw(sheet)
    for row,i in enumerate(indices):
        for column,arm in enumerate(['qml','bead']):
            ims,values=arms[arm];j=min(i,len(ims)-1);y=row*(h+22)
            draw.text((column*w+4,y+3),f'{arm.upper()} frame {j}: proxy {values[j]["capture_proxy_ms"]} ms',fill='white')
            sheet.paste(ims[j],(column*w,y+22))
    sheet.save(out/f'series-{name}.png')
meta.update(json.loads((source/'source-revisions.json').read_text()))
meta['qml_revision']='ab04e9380ab039aa3fc6bf1d87c30703c2b8b35c'
meta['title_retention']=json.loads((source/'bead/title-proof.json').read_text())
meta['containment']={}
for p in sorted((base/'evidence').glob('*-containment.json')):
    if p.name.startswith('final-memory-') or p.name=='parity-containment.json':
        value=json.loads(p.read_text())
        meta['containment'][p.name]={k:value.get(k) for k in ['capture_returncode','child_membership_verified','effective_memory_max','effective_memory_swap_max','oom_delta']}
(out/'capture-metadata.json').write_text(json.dumps(meta,indent=2)+'\n')
print(json.dumps({k:{'frames':len(v),'centers':sorted(set(r['pearl_x'] for r in v if r['pearl_x'] is not None))} for k,v in meta['series'].items()},indent=2))
