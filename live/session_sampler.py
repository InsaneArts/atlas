import subprocess,time,json,plistlib,pathlib,datetime
out=pathlib.Path(__file__).resolve().parents[1]/'evidence/live/session-resources.jsonl'
end=time.monotonic()+1800
while time.monotonic()<end:
 data=plistlib.loads(subprocess.check_output(['/usr/sbin/ioreg','-n','Root','-d','1','-a']))
 raw=subprocess.check_output(['/bin/ps','-axo','uid,pid,rss,pcpu,comm'],text=True)
 helper=[]
 for line in raw.splitlines()[1:]:
  fields=line.strip().split(None,4)
  if fields and fields[0]=='502':helper.append({'pid':int(fields[1]),'rssKiB':int(fields[2]),'cpuPercent':float(fields[3]),'process':fields[4]})
 row={'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'locked':data.get('IOConsoleLocked'),'sessions':[{k:v for k,v in s.items() if k in ['kCGSSessionAuditIDKey','kCGSSessionIDKey','kCGSSessionOnConsoleKey','kCGSSessionUserIDKey','kCGSessionLoginDoneKey','CGSSessionUniqueSessionUUID']} for s in data.get('IOConsoleUsers',[])],'helperRSSSumKiB':sum(p['rssKiB'] for p in helper),'helperProcessCount':len(helper),'helperProcesses':helper}
 with out.open('a') as f:f.write(json.dumps(row)+'\n')
 time.sleep(10)
