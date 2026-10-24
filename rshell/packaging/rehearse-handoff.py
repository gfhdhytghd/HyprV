#!/usr/bin/env python3
"""No-root simulation of the staged system switch's actual user-preflight handoff."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import pwd
import subprocess
from unittest import mock

ROOT=Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    if os.geteuid()==0:raise RuntimeError('simulation must run as desktop user, without root')
    release=ROOT/'packaging/dist/release'
    spec=importlib.util.spec_from_file_location('handoff_staged_control',release/'cutover/control.py')
    control=importlib.util.module_from_spec(spec);spec.loader.exec_module(control)
    assert control.STAGED and control.TREE==ROOT.resolve()
    assert pwd.getpwnam(control.OWNER).pw_uid==os.getuid()
    captured=[];result=None
    class EndSimulation(Exception):pass
    def user_preflight_only(cmd,**kwargs):
        nonlocal result
        command=list(map(str,cmd));expected=list(map(str,control.owner_preflight_command()))
        assert command==expected
        # Already running as OWNER: never execute runuser or any root command.
        user_command=[control.PYTHON,'-B',str(control.TREE/'cutover/preflight.py'),'--installed','--json']
        result=subprocess.run(user_command,cwd='/',text=True,capture_output=True,timeout=120)
        captured.append({'root_handoff_command':command,'executed_as_current_user':user_command})
        raise EndSimulation()
    with mock.patch.object(control.os,'geteuid',return_value=0),mock.patch.object(control,'verify_staged',return_value='simulated-trust-only'),mock.patch.object(control,'call',side_effect=user_preflight_only):
        try:control.root_handoff('switch')
        except EndSimulation:pass
    assert len(captured)==1 and result is not None
    gates=json.loads(result.stdout)
    assert next(row for row in gates['checks']if row['check']=='release paths')['ok']
    receipt={'release_id':__import__('hashlib').sha256((release/'MANIFEST.sha256').read_bytes()).hexdigest(),
             'tree':str(control.TREE),'root':False,'privileged_commands_executed':False,
             'root_ownership_verification_simulated':True,'handoff':captured[0],
             'user_preflight_returncode':result.returncode,'user_preflight':gates,'stderr':result.stderr}
    args.out.parent.mkdir(parents=True,exist_ok=True);args.out.write_text(json.dumps(receipt,indent=2)+'\n')
    print('PASS: staged root_handoff resolves TREE='+str(control.TREE)+' and executes the actual installed-user preflight as '+control.OWNER+' (exit '+str(result.returncode)+'; gate blockers retained)')
if __name__=='__main__':main()
