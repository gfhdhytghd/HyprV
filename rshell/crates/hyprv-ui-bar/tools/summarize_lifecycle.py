#!/usr/bin/env python3
"""Validate lifecycle receipts, correlate every captured buffer with its pre-swap audit.
The rich profile retains its fixed CPU delta; its independent fresh-GPU control must
still match every byte. Unread bursts are reported separately, never as full readback.
"""
from pathlib import Path
import collections, gzip, hashlib, json, re
ROOT=Path(__file__).resolve().parents[3]
ART=ROOT/'crates/hyprv-ui-bar/artifacts'
NAMES=['gpu-risk-final-nvidia-rates','gpu-risk-final-nvidia-hotplug',
       'gpu-risk-final-rich','gpu-risk-final-unfenced','gpu-risk-final-restart',
       'gpu-risk-final-mesa','gpu-risk-final-skia-nvidia','gpu-risk-final-skia-mesa']
rows=[];totals=collections.Counter();transports=[]
for name in NAMES:
    base=ART/name;r=json.loads((base/'receipt.json').read_text())
    assert not r['source_drift'],(name,r['source_drift'])
    counts=collections.Counter();audits=0;prefix=0;cpu_max=0;mismatches=0
    rates=set();robust=False;drivers=set()
    for epoch in r['epochs']:
        assert epoch['returncode']==0,(name,epoch)
        folder=base/f"epoch-{epoch['epoch']}"
        logpath=base/f"epoch-{epoch['epoch']}.log"
        raw=logpath.read_text(errors='replace') if logpath.exists() else gzip.decompress(Path(str(logpath)+'.gz').read_bytes()).decode(errors='replace')
        pairs=re.findall(r'GPU_SELF_CHECK exact source=([0-9a-f]+) window=([0-9a-f]+)',raw)
        assert all(a==b for a,b in pairs);audits+=len(pairs)
        if epoch['result']:
            result=epoch['result'];counts.update(result['counts']);drivers.add(result['driver'])
            assert not result['mismatches'];mismatches+=len(result['mismatches'])
            frames=json.loads((folder/'frames.json').read_text())
            assert len(frames)==result['counts']['checked']
            assert all(f['lifecycle_control_exact']for f in frames)
            cpu_max=max(cpu_max,max((f['max_delta']for f in frames),default=0))
            if not r['args']['no_readback']:
                assert len(frames)==result['counts']['submitted']
                calibration=56 if r['args']['rich']else 0
                assert len(pairs)==calibration+len(frames)
                assert [x[0]for x in pairs[calibration:]]==[f['source_checksum']for f in frames]
            assert cpu_max<=(1 if r['args']['rich']else 0),(name,cpu_max)
            rates.update(m['refresh_mHz']for m in result['output_mode_events'])
            robust|='actual_GL_CONTEXT_ROBUST_ACCESS=1'in result['extensions']
            if r['args']['dual_rates']:assert {60000,144000}.issubset(rates)
            if 'skia-'not in name:assert result['max_simultaneous_outputs']==2
        else:
            assert epoch['expected_disconnect']and epoch['actual_returncode']==1
            frames=json.loads((folder/'frames-checkpoint.json').read_text())
            assert all(f['lifecycle_control_exact']and f['max_delta']==0 for f in frames)
            assert [x[0]for x in pairs[:len(frames)]]==[f['source_checksum']for f in frames]
            prefix+=len(frames)
        if 'wl_surface#'in raw or 'wl_surface@'in raw:
            t=collections.Counter();pending=set()
            for line in raw.splitlines():
                if m:=re.search(r'new id wl_buffer[#@](\d+)',line):pending.discard(int(m[1]));t['created']+=1
                if m:=re.search(r'wl_surface[#@]\d+\.attach\(wl_buffer[#@](\d+)',line):
                    b=int(m[1]);t['attachments']+=1
                    if b in pending:t['reattach_before_observed_release']+=1
                    pending.add(b)
                if m:=re.search(r'wl_buffer[#@](\d+)\.release\(',line):pending.discard(int(m[1]));t['releases']+=1
                if ' -> 'in line and (m:=re.search(r'wl_buffer[#@](\d+)\.destroy\(',line)):
                    b=int(m[1]);t['destroys']+=1
                    if b in pending:t['proxy_destroyed_while_server_may_hold_buffer']+=1
                    pending.discard(b)
            t.setdefault('reattach_before_observed_release',0)
            transports.append({'run':name,'epoch':epoch['epoch'],'counts':dict(t),
                'log_sha256':hashlib.sha256(raw.encode()).hexdigest(),
                'note':'Client proxy destruction before release is legal; the compositor keeps its buffer reference. Counters alone do not prove hardware presentation.'})
    totals.update(counts)
    rows.append({'run':name,'binary_sha256':r['binary_sha256'],'driver':sorted(drivers),
                 'counts':dict(counts),'pre_swap_audits':audits,'verified_before_disconnect':prefix,
                 'cpu_max_delta':cpu_max,'lifecycle_mismatches':mismatches,
                 'robust_context_observed':robust,'advertised_refresh_mHz':sorted(rates),
                 'source_drift':r['source_drift']})
assert len({r['binary_sha256']for r in rows})==1
summary={'runs':rows,'complete_run_counts':dict(totals),
         'pre_swap_audits':sum(r['pre_swap_audits']for r in rows),
         'verified_before_disconnect':sum(r['verified_before_disconnect']for r in rows),
         'lifecycle_mismatches':0,'transport':transports,
         'scope':'Private headless/nested sessions. Real KMS VT switch, device reset and hardware scanout were not induced.'}
(ART/'gpu-risk-final-summary.json').write_text(json.dumps(summary,indent=2))
print(json.dumps({k:v for k,v in summary.items()if k not in ['runs','transport']},indent=2))
