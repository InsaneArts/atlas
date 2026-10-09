#!/usr/bin/env python3
import argparse,base64,datetime,fcntl,json,os,pathlib,socket,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
EVIDENCE=ROOT/'evidence'/'live'
EVIDENCE.mkdir(exist_ok=True)
class Adapter:
 def __init__(self,uid):
  self.uid=int(uid)
  if self.uid<=501: raise RuntimeError('Refusing human/system session as target')
  self.path=f'/tmp/atlas-live-{self.uid}/{self.uid}.sock'
 def rpc(self,request,label=None,timeout=45):
  started=time.perf_counter_ns(); wall=datetime.datetime.now(datetime.timezone.utc).isoformat(); records=[]
  try:
   with socket.socket(socket.AF_UNIX) as s:
    s.settimeout(timeout); s.connect(self.path); s.sendall((json.dumps(request)+'\n').encode())
    with s.makefile('rb') as f:
     for line in f:
      row=json.loads(line); records.append(row)
      if 'ok' in row: break
   if not records or 'ok' not in records[-1]: raise RuntimeError('No final RPC response')
   final=records[-1]
  except Exception as exc:
   final={'ok':False,'transportError':str(exc)}
  elapsed=(time.perf_counter_ns()-started)/1e6
  log={'timestamp':wall,'label':label,'socket':self.path,'request':request,'elapsedMs':elapsed,'response':{k:v for k,v in final.items() if k!='png'}}
  if 'png' in final:
   raw=base64.b64decode(final['png']); name=f'{time.time_ns()}-{label or "capture"}.png'; path=EVIDENCE/name; path.write_bytes(raw)
   log['screenshot']=str(path); final['path']=str(path); del final['png']
  log['events']=records[:-1]
  with (EVIDENCE/'rpc.jsonl').open('a') as f: f.write(json.dumps(log)+'\n')
  final['elapsedMs']=elapsed
  if records[:-1]: final['events']=records[:-1]
  return final
 def identity(self):
  status=self.rpc({'op':'hello'},'identity')
  if not status.get('ok') or status['user']['uid']!=self.uid or status['session']['onConsole'] or status['display']['width']<=0:
   raise RuntimeError(f'Hidden session precondition failed: {status}')
  return status
 def input(self,actions,label):
  with (EVIDENCE/f'controller-{self.uid}.lock').open('w') as f:
   fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
   self.identity()
   return self.rpc({'op':'input','actions':actions},label)
 def capture(self,label,maximum=None):
  self.identity(); req={'op':'screenshot'}
  if maximum: req['maxDimension']=maximum
  return self.rpc(req,label)
 def run(self,argv,label='run'):
  self.identity(); return self.rpc({'op':'run','argv':argv,'timeoutMs':20000},label)
if __name__=='__main__':
 p=argparse.ArgumentParser(); p.add_argument('uid',type=int); p.add_argument('request'); p.add_argument('--label',default='manual'); args=p.parse_args()
 a=Adapter(args.uid); req=json.loads(args.request)
 if req['op']=='input': result=a.input(req['actions'],args.label)
 elif req['op']=='screenshot': result=a.capture(args.label,req.get('maxDimension'))
 elif req['op']=='run': result=a.run(req['argv'],args.label)
 else: result=a.rpc(req,args.label)
 print(json.dumps(result,indent=2))
