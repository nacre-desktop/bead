#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Unchanged NacreAppBrowser with exact fixture replies, no provider engine."""
import importlib.util,json,os,pathlib,re,shutil,subprocess,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('matrix',ROOT/'spikes/bar-matrix.py')
matrix=importlib.util.module_from_spec(spec);spec.loader.exec_module(matrix)
spec=importlib.util.spec_from_file_location('menu_fixtures',ROOT/'spikes/menu-fixtures.py')
menu_fixtures=importlib.util.module_from_spec(spec);spec.loader.exec_module(menu_fixtures)
def sample(env,raw,compositor,stop):
    env={**env,'DBUS_SYSTEM_BUS_ADDRESS':env['DBUS_SESSION_BUS_ADDRESS'],'PIPEWIRE_REMOTE':'bead-private-unavailable'}
    if compositor=='hyprland':
        for command,args in [('dismissnotify',['-1']),('seterror',['disable'])]:subprocess.run(['hyprctl',command,*args],env=env,capture_output=True,check=True,timeout=5)
    with tempfile.TemporaryDirectory(prefix='bead-launcher-reference-')as directory:
        root=pathlib.Path(directory);original=matrix.reference.fixture(root)
        shutil.copytree(original/'modules/launcher',root/'modules/launcher')
        shutil.copytree(original/'modules/extras',root/'modules/extras')
        data=[{'id':f'proof-{i:04}','name':f'Application {i:04}','label':f'Application {i:04}','icon':str(ROOT/'shell/sample.svg'),'kind':'app','description':'Application fixture','breadcrumb':'All apps','operation':'app','disabled':False}for i in range(320)]
        menu="""pragma Singleton
import QtQuick
QtObject {
    property var navigation:[{id:"home",label:"Applications",icon:"apps"},{id:"search",label:"Search",icon:"search"},{id:"actions",label:"Actions",icon:"terminal"}]
    property var rows:navigation
    property var fixture: __FIXTURE__
    property var sourceOptions:({actions:true,calculator:false,emoji:true,clipboard:true,brain:true,files:true,ai:false})
    property var searchFilters:[{value:"all",label:"All"}]
    property var softwareCategories:[]
    property var ownership:({})
    function recent(){return []}
    property var packageNames:({})
    property string message:""
    property string packageMessage:""
    property string fileMessage:""
    property string conversionQuery:""
    property var conversion:({ok:false})
    property var catalog:({error:""})
    property QtObject library:QtObject {property string message:"";property var data:({});signal completed(string operation,bool success);function work(){return false}}
    property QtObject workflows:QtObject {property string message:"";property var data:({roots:[]});property bool recording:false;signal completed(string operation,bool success);function startRecording(){}function work(){return false}}
    property QtObject previews:QtObject {property var data:({});property var preview:({image:""});property string previewPath:"";property bool busy:false;property string message:"";function inspect(value){} }
    function content(route,query,filter,category) {const result=fixture.queries[query]; return result ? result.order.map(id=>Object.assign({id:id,operation:result.items[id].kind},result.items[id])) : []}
    function resolve(route){return route}
    function node(id){return rows.find(row=>row.id===id)||null}
    function prepare(){}
    function keepOpen(){return false}
    function execute(){}
    function inspectApp(){}
    function itemActions(){return []}
    function supportPackage(){return ""}
    function stateFor(){return ""}
    function work(){return false}
}
"""
        replies={'':{'rows':data},'a':{'rows':data},'ap':{'rows':data}}
        changed=os.environ.get('BEAD_TYPING_CHANGED_RESULTS')=='1'
        if changed:
            replies['a']={'rows':data[:160]};replies['ap']={'rows':data[160:]}
        menu=menu.replace('__FIXTURE__',json.dumps({'queries':{q:menu_fixtures.reply(q,r['rows'])for q,r in replies.items()}}))
        (root/'services/NacreMenu.qml').write_text(menu)
        for name,body in {
            'NacreControlTools':'function run(){} function work(){}',
            'NacreMenuPrompt':'property bool active:false;property var ownerView:null;property var request:null;function dismiss(){}',
            'LauncherPreferences':'property var favorites:[];property var hidden:[];function update(){}',
            'NacreI18n':'function tr(key,fallback,args){return typeof fallback==="string"?fallback:fallback.other} function number(value){return String(value)} function option(domain,key){return String(key)} function windowCaption(title){return title}',
            'NacreWallpapers':'property var appearancePreferences:({paletteMode:"dark"});property var wallpapers:[]; property string current:"";function set(){}',
            'NacrePanelState':'function activeName(){return "private"} function close(){} function openMode(){}',
        }.items():
            (root/'services'/(name+'.qml')).write_text('pragma Singleton\nimport QtQuick\nQtObject {'+body+'}\n')
        shell_source=("""import QtQuick
import Quickshell
import Quickshell.Wayland
import "modules/launcher"
import "services"
ShellRoot {
    QtObject {id:visibility;property bool launcher:true;property bool reveal:false;property string launcherMode:"apps";property string launcherRoute:"home";property int launcherRequest:0;property string query:"";}
    PanelWindow {
        anchors.top:true;implicitWidth:1164;implicitHeight:654;exclusiveZone:-1;color:"transparent"
        WlrLayershell.namespace:"bead-reference-launcher"
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
        Rectangle {x:32;y:32;width:1100;height:590;radius:12;color:NacreColours.palette.m3surface;border.width:1;border.color:NacreColours.palette.m3outlineVariant}
        NacreAppBrowser {id:browser;x:32;y:32;width:1100;height:590;viewportWidth:1920;viewportHeight:1200;visibilities:visibility;menu:NacreMenu;Component.onCompleted:focusExplicit()}
        Timer {interval:2500;running:true;onTriggered:console.warn("LAUNCHER_QML_READY entries="+browser.entries.length)}
    }
}
""")
        typing=os.environ.get('BEAD_QML_TYPING')=='1'
        client_env=dict(env)
        if typing:
            probe=os.environ['BEAD_QML_PROBE']
            client_env.update(LD_PRELOAD=probe,BEAD_QML_TRACE_INPUT='1',BEAD_QML_QUIET_IDLE='1')
            shell_source=shell_source.replace('import QtQuick','import QtQuick\nimport BeadProbe 1.0',1).replace('        Timer {interval:2500','        Connections {target:browser;function onQueryChanged(){TimingProbe.inputCommitted()}}\n        Timer {interval:2500')
        (root/'shell.qml').write_text(shell_source)
        client=keeper=None;log=raw/'launcher-qml.log'
        try:
            if typing:
                keeper=subprocess.Popen(['wtype','-s','600000',''],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
                time.sleep(.1)
                if keeper.poll() is not None:raise RuntimeError('virtual keyboard keeper exited')
            with log.open('w')as stream:
                client=subprocess.Popen(['quickshell','--path',str(root/'shell.qml')],cwd=ROOT,env=client_env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
                time.sleep(3)
                subprocess.run(['grim',str(raw/'launcher-initial.png')],env=env,capture_output=True,check=True,timeout=8)
                matrix.wait_log(log,lambda t:'LAUNCHER_QML_READY entries=320'in t,timeout=15)
                if typing:
                    if compositor=='hyprland':subprocess.run(['hyprctl','eval','hl.dispatch(hl.dsp.focus({monitor="bead-bench"}))'],env=env,capture_output=True,check=True,timeout=5)
                    rows=[]
                    for index in range(int(os.environ.get('BEAD_TYPING_KEYS','200'))):
                        offset=len(matrix.log_text(log));key=['a']if index==0 else ['p']if index%2 else['-k','BackSpace']
                        subprocess.run(['wtype','-s','20',*key],env=env,capture_output=True,check=True,timeout=5)
                        matrix.wait_log(log,lambda t:re.search(r'QML_UI_PRESENTED generation=\d+ monotonic_ns=\d+ cpu_ns=\d+',t[offset:])is not None,timeout=8)
                        segment=matrix.log_text(log)[offset:]
                        receipt=re.findall(r'QML_INPUT_RECEIVED generation=(\d+) monotonic_ns=(\d+) cpu_ns=(\d+)',segment)
                        assert len(receipt)==1,'expected one QML key receipt'
                        generation,stamp,cpu=map(int,receipt[0])
                        assert re.search(r'QML_INPUT_COMMIT generation='+str(generation)+r' ',segment),'query did not commit'
                        presented=re.findall(r'QML_UI_PRESENTED generation=(\d+) monotonic_ns=(\d+) cpu_ns=(\d+)',segment)
                        exact=[(int(ns),int(end))for gen,ns,end in presented if int(gen)==generation and int(ns)>=stamp]
                        assert exact,'missing exact QML query presentation'
                        ns,end=exact[0];rows.append({'index':index,'generation':generation,'latency_ms':(ns-stamp)/1e6,'cpu_ms':(end-cpu)/1e6})
                        time.sleep(.04)
                    assert 'QML_PRESENTATION clock_id=1'in log.read_text()
                    summary={'compositor':compositor,'keys':len(rows),'catalog_size':320,'source':'exact fixture replies; no engine','helper_cpu_ms':None,'changed_result_ids':changed,'fixture_result_counts':{q:len(r['rows'])for q,r in replies.items()},'rows':rows}
                    for metric in ['cpu_ms','latency_ms']:
                        values=[r[metric]for r in rows];summary[metric]={**matrix.percentiles(values),'mean':sum(values)/len(values),'max':max(values)}
                    (raw/'qml-typing-proof.json').write_text(json.dumps(summary,indent=2)+'\n')
                    print('QML_TYPING_PROOF keys='+str(len(rows))+' cpu_mean_ms='+str(summary['cpu_ms']['mean'])+' presented_p99_ms='+str(summary['latency_ms']['p99']),flush=True)
                seconds=float(os.environ.get('BEAD_REFERENCE_SECONDS','60'))
                result=matrix.idle(client.pid,seconds)
                subprocess.run(['grim',str(raw/'launcher-qml.png')],env=env,capture_output=True,check=True,timeout=8)
                result.update(compositor=compositor,component='unchanged NacreAppBrowser',fixture='exact precomputed menu.content replies; no catalog/ranking/providers; details pane retained')
                (raw/'launcher-qml.json').write_text(json.dumps(result,indent=2)+'\n')
                print('QML_LAUNCHER_REFERENCE pss_plus_drm_mib='+str(result['pss_plus_drm_mib']),flush=True)
        finally:
            if client:stop(client)
            if keeper:stop(keeper)
