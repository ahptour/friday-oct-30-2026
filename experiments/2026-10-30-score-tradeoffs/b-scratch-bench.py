import hashlib,json,statistics,subprocess,tempfile
from pathlib import Path
root=Path.cwd();work=root/'.out/b-next'
expected={'b':'f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927','c':'d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536','d':'a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e'}
results={}
for track,variants in [('b',['base','both'])]:
 # Prewarm all mapped data in an independent cgroup before measured processes.
 subprocess.run(['docker','run','--rm','--network=none','-v',f'{root}/data/{track}/full-seed1:/data:ro','--entrypoint','/bin/sh',f'speeddemon-b-next/{variants[0]}','-c','cat /data/* >/dev/null'],check=True)
 for trial in range(9):
  for variant in (variants if trial%2==0 else variants[::-1]):
   out=Path(tempfile.mkdtemp(dir=work));out.chmod(0o777)
   cmd=['docker','run','--rm','--network=none','--read-only','--tmpfs','/tmp:rw,size=1g,exec','--memory=8g','--memory-swap=8g','--pids-limit=512','--cap-drop=ALL','--security-opt=no-new-privileges','--user','65534:65534','--cpuset-cpus=0-11','-v',f'{root}/data/{track}/full-seed1:/data:ro','-v',f'{out}:/out:rw','-v',f'{root}/.out/score-tradeoffs/timer:/timer:ro','--entrypoint','/timer',f'speeddemon-b-next/{variant}']
   subprocess.run(['docker','run','--rm','--network=none','-v',f'{root}/data/{track}/full-seed1:/data:ro','--entrypoint','/bin/sh',f'speeddemon-b-next/{variant}','-c','cat /data/* >/dev/null'],check=True)
   p=subprocess.run(cmd,capture_output=True,text=True,check=True)
   seconds,memory=p.stdout.split(); files=list(out.iterdir());assert len(files)==1
   digest=hashlib.sha256(files[0].read_bytes()).hexdigest();assert digest==expected[track],digest
   row={'trial':trial,'seconds':float(seconds),'memory':int(memory),'sha256':digest}
   results.setdefault(f'{track}-{variant}',[]).append(row)
   print(track,variant,trial,seconds,memory,flush=True)
   work.joinpath('bench-prewarm.json').write_text(json.dumps(results,indent=2))
for k,v in results.items():
 warm=v[1:]; print(k,'median_s',statistics.median(x['seconds'] for x in warm),'max_memory',max(x['memory'] for x in warm),flush=True)
