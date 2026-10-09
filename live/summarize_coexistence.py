#!/usr/bin/env python3
"""Summarize retained live measurements; no generated/simulated observations."""
import datetime,hashlib,json,math,pathlib,statistics
from PIL import Image
ROOT=pathlib.Path(__file__).resolve().parents[1]
P=ROOT/'evidence/live'
def read(name):return json.loads((P/name).read_text())
def rows(name):return [json.loads(l) for l in (P/name).read_text().splitlines()]
def date(s):return datetime.datetime.fromisoformat(s.replace('Z','+00:00'))
def stats(values):
 v=sorted(values)
 return {'count':len(v),'median':statistics.median(v),'p95NearestRank':v[math.ceil(.95*len(v))-1],'min':min(v),'max':max(v)} if v else None
result=read('coexistence-result.json')
start=date(result['startedAt']);end=date(result['finishedAt'])
cycles=rows('coexistence-cycles.jsonl')
rpc=[r for r in rows('rpc.jsonl') if (r.get('label') or '').startswith('coexist-') and start<=date(r['timestamp'])<=end]
inputs=[r for r in rpc if r['request']['op']=='input']
shots=[r for r in rpc if r['request']['op']=='screenshot']
actions={}
for r in inputs:
 for a in r['request']['actions']:actions[a['type']]=actions.get(a['type'],0)+1
host=[r for r in rows('host-monitor.jsonl') if start<=date(r['time'])<=end]
windows=[r for r in rows('host-window-monitor.jsonl') if start<=date(r['timestamp'])<=end]
resources=[r for r in rows('session-resources.jsonl') if start<=date(r['timestamp'])<=end]
helperPIDs={31563,32746,32831,31400}
crop=(115,590,950,635);clock=[]
for r in shots:
 with Image.open(r['screenshot']) as im:
  clock.append({'file':pathlib.Path(r['screenshot']).name,'size':im.size,'clockRGBsha256':hashlib.sha256(im.convert('RGB').crop(crop).tobytes()).hexdigest()})
sampledHelper=[]
for r in resources:
 sampledHelper.extend(p for p in r['helperProcesses'] if p['process']=='/Users/Shared/AtlasLiveTest/offstage-sessiond')
summary={
 'run':result,'verifiedCycles':len(cycles),'inputRPCs':len(inputs),'inputRPCFailures':[r for r in inputs if not r['response'].get('ok')],
 'actionsByType':actions,'inputRPCMs':stats([r['elapsedMs'] for r in inputs]),
 'verifiedClickResponseMs':stats([r['clickResponseMs'] for r in cycles]),
 'fullCaptureMs':stats([r['elapsedMs'] for r in shots]),
 'captureMethod':'Full PNG Unix RPC elapsed, including wire receipt/JSON decoding; identity preflight and PNG decode/file write excluded.',
 'responseMethod':'Click request until fixture state-file read verifies the increment; includes identity checks and read RPC, not display scanout latency.',
 'screenshots':clock,'clockCrop':crop,'consecutiveClockRegionsChanged':all(a['clockRGBsha256']!=b['clockRGBsha256'] for a,b in zip(clock,clock[1:])),
 'host':{'samples':len(host),'firstAt':host[0]['time'],'lastAt':host[-1]['time'],
  'allOnConsole':all(r['onConsole'] is True for r in host),'frontmostPIDs':sorted(set(r['frontmostPID'] for r in host)),
  'focusChanges':sum(a['frontmostPID']!=b['frontmostPID'] for a,b in zip(host,host[1:])),
  'helperFrontmostSamples':sum(r['frontmostPID'] in helperPIDs for r in host),
  'pointerChangedSamples':sum((a['cursorX'],a['cursorY'])!=(b['cursorX'],b['cursorY']) for a,b in zip(host,host[1:])),
  'pointerAtExactInjectionCoordinatesSamples':sum((r['cursorX'],r['cursorY']) in [(260,1030),(240,750),(400,1040)] for r in host),
  'clipboardChangeCountBefore':host[0]['clipboardChangeCount'],'clipboardChangeCountAfter':host[-1]['clipboardChangeCount'],
  'privacy':'No human keystrokes, clipboard contents, window titles or host screen images recorded. Activity telemetry does not prove continuous typing or assign every clipboard change.'},
 'windowMonitor':{'samples':len(windows),'firstAt':windows[0]['timestamp'],'lastAt':windows[-1]['timestamp'],
  'coverageSeconds':(date(windows[-1]['timestamp'])-date(windows[0]['timestamp'])).total_seconds(),
  'helperWindowMatches':sum(len(r['helperWindowsInHostSession']) for r in windows),'monitoredPIDs':sorted(helperPIDs)},
 'resources':{'samples':len(resources),'helperProcessCount':stats([r['helperProcessCount'] for r in resources]),
  'aggregateHelperRSSGiB':stats([r['helperRSSSumKiB']/1024**2 for r in resources]),
  'daemonRSSMiB':stats([r['rssKiB']/1024 for r in sampledHelper]),'daemonCPUPercentPs':stats([r['cpuPercent'] for r in sampledHelper]),
  'limitation':'Aggregate process RSS double-counts shared pages and is not incremental physical RAM. ps CPU samples are not a utilization trace. No clean pre-login baseline was measured.'}
}
(ROOT/'evidence/coexistence-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k!='screenshots'},indent=2))
