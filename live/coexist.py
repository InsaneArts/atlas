#!/usr/bin/env python3
import base64,datetime,json,os,pathlib,sys,time,statistics,math
from adapter import Adapter,EVIDENCE
DURATION=1800.0
uid=502
a=Adapter(uid)
path=EVIDENCE/'coexistence-cycles.jsonl'
progress=EVIDENCE/'coexistence-progress.json'
start=time.monotonic();wall=datetime.datetime.now(datetime.timezone.utc).isoformat()
cycles=[];error=None

def state(label):
 r=a.run(['/bin/cat','/Users/atlastest/Library/Logs/AtlasLiveTest/state.json'],label)
 if not r.get('ok') or r.get('exitCode')!=0:raise RuntimeError(f'Cannot read fixture state: {r}')
 s=''.join(base64.b64decode(e['data']).decode() for e in r.get('events',[]) if e.get('event')=='output')
 return json.loads(s)

def wait(ms):return {'type':'wait','ms':ms}
try:
 a.identity()
 a.run(['/usr/bin/open','-a','/Users/Shared/AtlasLiveTest/AtlasFixture.app'],'coexist-activate-fixture')
 time.sleep(.4)
 before=state('coexist-baseline')
 initial=a.capture('coexist-begin')
 i=0
 while time.monotonic()-start<DURATION:
  if (EVIDENCE/'STOP-COEXIST').exists():raise RuntimeError('Explicit stop file detected')
  status=a.identity()
  cycleStart=time.monotonic();targetClicks=before['clicks']+1
  r=a.input([{'type':'click','x':260,'y':1030}],f'coexist-{i:03d}-click')
  if not r.get('ok'):raise RuntimeError(f'Input refused: {r}')
  after=state(f'coexist-{i:03d}-verify-click')
  for retry in range(20):
   if after['clicks']==targetClicks:break
   time.sleep(.02);after=state(f'coexist-{i:03d}-verify-click-{retry}')
  if after['clicks']!=targetClicks:raise RuntimeError(f'Click verification mismatch cycle {i}: expected {targetClicks}, got {after["clicks"]}')
  clickMs=(time.monotonic()-cycleStart)*1000
  extra=None
  if i%3==0:
   marker=f'\nATLAS cycle {i:03d} ✓'
   extra=a.input([{'type':'click','x':240,'y':750},wait(100),{'type':'key','key':'cmd+end'},wait(80),{'type':'type','text':marker},wait(100)],f'coexist-{i:03d}-text')
   after=state(f'coexist-{i:03d}-verify-text')
   if not extra.get('ok') or marker not in after['text']:raise RuntimeError(f'Text verification failed cycle {i}')
  if i%6==0:
   previous=after.copy()
   extra=a.input([{'type':'move','x':400,'y':1040},wait(150),{'type':'scroll','dx':0,'dy':-2},wait(150),{'type':'scroll','dx':-2,'dy':0},wait(150)],f'coexist-{i:03d}-scroll')
   after=state(f'coexist-{i:03d}-verify-scroll')
   if after['scrollY']!=previous['scrollY']-2 or after['scrollX']!=previous['scrollX']-2:raise RuntimeError(f'Scroll verification failed cycle {i}')
  shot=a.capture(f'coexist-{i:03d}') if i%6==0 else None
  row={'cycle':i,'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsedSeconds':time.monotonic()-start,'status':status,'clickResponseMs':clickMs,'clickRpcMs':r['elapsedMs'],'clicks':after['clicks'],'textLength':len(after['text']),'scrollX':after['scrollX'],'scrollY':after['scrollY'],'screenshot':shot,'verified':True}
  cycles.append(row)
  with path.open('a') as f:f.write(json.dumps(row)+'\n')
  progress.write_text(json.dumps({'startedAt':wall,'elapsedSeconds':time.monotonic()-start,'durationSeconds':DURATION,'completedCycles':len(cycles),'lastCycle':row},indent=2))
  print(json.dumps({'cycle':i,'elapsedSeconds':round(time.monotonic()-start,1),'verified':True}),flush=True)
  before=after;i+=1
  nextTime=start+i*5
  while time.monotonic()<min(nextTime,start+DURATION):time.sleep(min(.5,min(nextTime,start+DURATION)-time.monotonic()))
except Exception as e:
 error=repr(e);print('STOPPED:',error,flush=True)
finally:
 elapsed=time.monotonic()-start
 result={'startedAt':wall,'finishedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsedSeconds':elapsed,'targetSeconds':DURATION,'verifiedCycles':len(cycles),'error':error,'completedDuration':elapsed>=DURATION and error is None,'scope':'Live hidden fixture input with state-file verification; screenshots retained every sixth cycle. Human activity and noninterference require separate host telemetry and human observations.'}
 if cycles:
  values=sorted(c['clickResponseMs'] for c in cycles);result['clickResponseMedianMs']=statistics.median(values);result['clickResponseP95MsNearestRank']=values[math.ceil(len(values)*.95)-1]
 (EVIDENCE/'coexistence-result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2),flush=True)
