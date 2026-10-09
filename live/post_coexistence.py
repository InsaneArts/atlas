#!/usr/bin/env python3
"""Actual competing-controller and in-flight restart tests in helper fixture."""
import concurrent.futures,json,threading,time
from adapter import Adapter,EVIDENCE
from recovery import ready,state,verify,stamp

run=json.loads((EVIDENCE/'coexistence-result.json').read_text())
assert run['completedDuration'], 'Do not interrupt the sustained run'
a=Adapter(502);ready(a)
a.run(['/usr/bin/open','-a','/Users/Shared/AtlasLiveTest/AtlasFixture.app'],'ownership-activate')
a.input([{'type':'click','x':240,'y':750},{'type':'wait','ms':150}],'ownership-focus')
before=state(a,'ownership-before')
barrier=threading.Barrier(2)
def client(char):
 c=Adapter(502);c.identity();barrier.wait()
 return c.rpc({'op':'input','actions':[{'type':'type','text':char*50}]},'ownership-client-'+('A' if char=='α' else 'B'))
start=time.monotonic()
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
 f1=pool.submit(client,'α');f2=pool.submit(client,'β');results=[f1.result(),f2.result()]
elapsed=time.monotonic()-start;time.sleep(.3)
after=state(a,'ownership-after');sequence=''.join(c for c in after['text'] if c in 'αβ')
ownership={'at':stamp(),'responses':results,'wallSeconds':elapsed,
 'beforeText':before['text'],'afterText':after['text'],'alphaBetaSequence':sequence,
 'alphaDelta':after['text'].count('α')-before['text'].count('α'),'betaDelta':after['text'].count('β')-before['text'].count('β'),
 'alternations':sum(x!=y for x,y in zip(sequence,sequence[1:])),
 'screenshot':a.capture('ownership-after'),
 'method':'Two independent real Unix clients, same hidden desktop. Intentionally bypasses only the cooperative Python adapter file lock, not any daemon permission or console guard.'}
(EVIDENCE/'ownership-result.json').write_text(json.dumps(ownership,indent=2)+'\n')
print(json.dumps({k:v for k,v in ownership.items() if k not in ['beforeText','afterText']},indent=2),flush=True)

ready(a);a.input([{'type':'click','x':240,'y':750},{'type':'wait','ms':150}],'restart-focus')
before=state(a,'restart-inflight-before');old=ready(a)
with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
 future=pool.submit(a.rpc,{'op':'input','actions':[{'type':'type','text':'R'*5000}]},'restart-inflight-type')
 time.sleep(.3);start=time.monotonic();restart=a.rpc({'op':'restart'},'restart-inflight-request')
 observations=[];new=None
 for _ in range(150):
  r=a.rpc({'op':'hello'},'restart-inflight-poll',2);observations.append({'elapsedMs':(time.monotonic()-start)*1000,'status':r})
  if r.get('ok') and r['daemon']['pid']!=old['daemon']['pid']:
   new=r;break
  time.sleep(.1)
 inputResult=future.result(timeout=15)
time.sleep(.3)
after=state(a,'restart-inflight-after')
result={'at':stamp(),'old':old,'new':new,'restart':restart,'inputResult':inputResult,'observations':observations,
 'observedReadyMs':observations[-1]['elapsedMs'] if new else None,'requestedCharacters':5000,
 'insertedRCount':after['text'].count('R')-before['text'].count('R'),
 'recovery':verify(a,'restart-inflight-recovery')}
(EVIDENCE/'restart-inflight-result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='observations'},indent=2),flush=True)
