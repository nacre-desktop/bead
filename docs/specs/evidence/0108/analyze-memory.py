from pathlib import Path
import json,statistics
base=Path(__file__).resolve().parent
out=base/'publish';out.mkdir(exist_ok=True)
keys=['Pss_Anon','Pss_Shmem','DRM','SwapPss','Pss_File','Pss']
result={'backend':'cpu','units':'MiB','definition':'unreclaimable = Pss_Anon + Pss_Shmem + DRM + SwapPss','pairs':[]}
for i in range(1,6):
    pair={}
    for role in ['control','candidate']:
        p=base/f'evidence/final-memory-{i}-{role}/bead/memory.json'
        m=json.loads(p.read_text());assert m['host_swap_in_pages']==0
        pair[role]={'revision':m['source_revision'],'phases':{phase:{k:statistics.median(row[k] for row in samples)/1024 for k in keys} for phase,samples in m['components_kib'].items()}}
        for phase in pair[role]['phases'].values():phase['unreclaimable']=sum(phase[k] for k in keys[:4])
    result['pairs'].append(pair)
result['phase_summary']={}
for phase in result['pairs'][0]['control']['phases']:
    deltas=[p['candidate']['phases'][phase]['unreclaimable']-p['control']['phases'][phase]['unreclaimable'] for p in result['pairs']]
    result['phase_summary'][phase]={'paired_deltas':deltas,'median_delta':statistics.median(deltas),'pass':statistics.median(deltas)<=.5}
result['pass']=all(p['pass'] for p in result['phase_summary'].values())
(out/'memory.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result['phase_summary'],indent=2)); print('PASS',result['pass'])
