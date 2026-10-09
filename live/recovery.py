#!/usr/bin/env python3
"""Coordinated real-desktop probes; never post input to an on-console session."""
import argparse,base64,concurrent.futures,datetime,json,pathlib,plistlib,subprocess,time
from adapter import Adapter,EVIDENCE

def stamp(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def output(r): return ''.join(base64.b64decode(e['data']).decode() for e in r.get('events',[]) if e.get('event')=='output')
def read(a,name,label):
 r=a.run(['/bin/cat','/Users/atlastest/Library/Logs/AtlasLiveTest/'+name],label)
 if not r.get('ok') or r.get('exitCode')!=0: raise RuntimeError(r)
 return output(r)
def state(a,label): return json.loads(read(a,'state.json',label))
def host():
 r=plistlib.loads(subprocess.check_output(['/usr/sbin/ioreg','-n','Root','-d','1','-a']))
 return {'time':stamp(),'locked':r.get('IOConsoleLocked'),'sessions':[{k:s.get(k) for k in ['kCGSSessionUserIDKey','kCGSSessionOnConsoleKey']} for s in r.get('IOConsoleUsers',[])]}
def ready(a):
 h=host()
 if h['locked']!=False or not any(s['kCGSSessionUserIDKey']==501 and s['kCGSSessionOnConsoleKey']==True for s in h['sessions']):
  raise RuntimeError('Human desktop is not unlocked and on console: '+str(h))
 return a.identity()
def verify(a,label):
 ready(a)
 b=state(a,label+'-before')
 r=a.input([{'type':'click','x':260,'y':1030}],label+'-click')
 for _ in range(30):
  time.sleep(.03); s=state(a,label+'-state')
  if s['clicks']==b['clicks']+1: break
 shot=a.capture(label+'-after')
 return {'input':r,'beforeClicks':b['clicks'],'afterClicks':s['clicks'],'verified':s['clicks']==b['clicks']+1,'screenshot':shot}
def arm(a,label,seconds=120):
 status=ready(a);pid=status['daemon']['pid']
 path='/Users/atlastest/Library/Logs/AtlasLiveTest/watch-'+label+'.jsonl'
 command=f'/Users/Shared/AtlasLiveTest/TransitionWatch {pid} {int(seconds)} --armed > {path} 2>&1 < /dev/null &'
 r=a.run(['/bin/sh','-c',command],label+'-arm-watch')
 time.sleep(.15)
 log=read(a,'watch-'+label+'.jsonl',label+'-watch-start')
 rows=[json.loads(l) for l in log.splitlines()]
 if not rows or rows[0]['event']!='start' or any(x['event']=='refused-to-arm' for x in rows):raise RuntimeError(log)
 return {'pid':pid,'status':status,'watchStart':rows,'launch':r}

def watch_transition(label,long_input=False):
 a=Adapter(502);ready(a)
 if long_input:
  a.run(['/usr/bin/open','-a','/Users/Shared/AtlasLiveTest/AtlasFixture.app'],label+'-activate')
  a.input([{'type':'click','x':240,'y':750},{'type':'wait','ms':150}],label+'-focus-text')
 b=state(a,label+'-before'); baselineEvents=read(a,'events.jsonl',label+'-baseline-events')
 seconds=240
 result={'case':label,'startedAt':stamp(),'beforeTextLength':len(b['text']),'observerLimitSeconds':seconds-5,'watch':arm(a,label,seconds),'humanSessionSamples':[],'helperSamples':[]}
 pool=concurrent.futures.ThreadPoolExecutor(max_workers=1)
 future=None
 if long_input:
  text='[ATLAS TRANSITION START]'+('T'*40000)+'[ATLAS TRANSITION END]'
  result['requestedTextLength']=len(text)
  future=pool.submit(a.rpc,{'op':'input','actions':[{'type':'type','text':text}]},label+'-long-input',seconds+30)
 (EVIDENCE/(label+'-armed.json')).write_text(json.dumps(result,indent=2))
 print('ARMED '+label,flush=True)
 start=time.monotonic();seen=False;returned=False;guard=None;firstChanged=None;seenHelperConsole=False
 try:
  while time.monotonic()-start<seconds-5:
   h=host();result['humanSessionSamples'].append(h)
   r=a.rpc({'op':'hello'},label+'-poll',3)
   sample={'time':stamp(),'status':r};result['helperSamples'].append(sample)
   changed=h['locked'] is True or any(s['kCGSSessionUserIDKey']==502 and s['kCGSSessionOnConsoleKey']==True for s in h['sessions'])
   if changed:
    if not seen:firstChanged=time.monotonic();result['firstTransitionObservedAt']=stamp()
    seen=True
   if r.get('ok') and r['session']['onConsole'] and guard is None:
    # Empty text exercises the real pre-action guard without creating events.
    guard=a.rpc({'op':'input','actions':[{'type':'type','text':''}]},label+'-on-console-empty-guard')
    result['onConsoleGuard']=guard
   if r.get('ok') and r['session']['onConsole']:seenHelperConsole=True
   humanBack=h['locked'] is False and any(s['kCGSSessionUserIDKey']==501 and s['kCGSSessionOnConsoleKey']==True for s in h['sessions'])
   if seen and humanBack and not changed and (not long_input or seenHelperConsole) and r.get('ok') and not r['session']['onConsole'] and time.monotonic()-firstChanged>1:
    returned=True;break
   time.sleep(.15)
  if future and not future.done():
   result['safetyStop']=a.rpc({'op':'restart'},label+'-safety-stop')
  if future:result['longInput']=future.result(timeout=12)
  result['transitionObserved']=seen;result['helperOnConsoleObserved']=seenHelperConsole;result['returnedToHuman']=returned
  # Only observe after proving the human is back and unlocked.
  if returned:
   for _ in range(80):
    try:ready(a);break
    except Exception:time.sleep(.25)
   time.sleep(.5)
   result['afterStatus']=ready(a)
   result['watchLog']=[json.loads(l) for l in read(a,'watch-'+label+'.jsonl',label+'-watch-final').splitlines()]
   events=read(a,'events.jsonl',label+'-events-final').splitlines()[len(baselineEvents.splitlines()):]
   result['fixtureEvents']=[json.loads(l) for l in events]
   s=state(a,label+'-after-transition');result['afterTextLength']=len(s['text'])
   result['textChangedOnConsole']=[e for e in result['fixtureEvents'] if e.get('event')=='textChanged' and e.get('sessionOnConsole') is True]
   result['recovery']=verify(a,label+'-recovery')
 except Exception as e:result['error']=repr(e)
 finally:
  pool.shutdown(wait=True)
  result['finishedAt']=stamp();(EVIDENCE/(label+'-result.json')).write_text(json.dumps(result,indent=2))
  print(json.dumps({k:v for k,v in result.items() if k not in ['fixtureEvents','humanSessionSamples','helperSamples','watch']},indent=2),flush=True)

if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('case',choices=['lock','sleep','switch']);args=p.parse_args()
 watch_transition(args.case,long_input=args.case=='switch')
