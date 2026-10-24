#!/usr/bin/env python3
"""Re-measure only a coherent staged production candidate; no install/cutover."""
import argparse, hashlib, importlib.util, json, os
from pathlib import Path
import signal, subprocess
ROOT=Path(__file__).resolve().parents[2]
def stop(*_):raise SystemExit(143)
signal.signal(signal.SIGTERM,stop)
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--tag',required=True);args=parser.parse_args()
    if not args.tag.replace('-','').isalnum():raise SystemExit('Invalid measurement tag')
    spec=importlib.util.spec_from_file_location('refresh_gate',ROOT/'cutover/preflight.py');gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
    receipt=json.loads((ROOT/'packaging/dist/build.json').read_text())
    if receipt['sources']!=gate.build.sources():raise SystemExit('Staged release sources changed; wait for a coherent release')
    archive=ROOT/'packaging/.build/perf-shell'/args.tag;archive.mkdir(parents=True,exist_ok=True)
    binary=archive/'hyprv-shell';binary.write_bytes((ROOT/'packaging/dist/hyprv-shell').read_bytes());binary.chmod(0o755)
    if hashlib.sha256(binary.read_bytes()).hexdigest()!=receipt['binaries']['hyprv-shell']:raise SystemExit('Staged release changed during archival')
    inputs=archive/'inputs.json';inputs.write_text(json.dumps({'command':receipt['command'],'source_root':str(ROOT),'files':gate.shell_sources()},indent=2)+'\n')
    env=os.environ.copy();env.update(HYPRV_SHELL_BINARY=str(binary),HYPRV_SHELL_SOURCE_ROOT=str(ROOT),HYPRV_SHELL_INPUTS_JSON=str(inputs))
    for suffix,options in [('quiet',['--quiet-agents','--idle-only']),('no-hooks',['--quiet-hooks','--idle-only']),('live',[])]:
        label=args.tag+'-'+suffix;log=ROOT/'docs/verify/raw/perf-shell'/(label+'-run.log')
        print('Real-shell refresh',label,flush=True)
        with log.open('w')as output:subprocess.run(['python3','-B','docs/verify/measure-shell.py','--label',label,*options],cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT,check=True)
    subprocess.run(['python3','-B','docs/verify/report-shell.py','--tag',args.tag],cwd=ROOT,check=True)
if __name__=='__main__':main()
