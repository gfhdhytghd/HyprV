#!/usr/bin/env python3
"""Summarize completed real-shell intervals; never installs or changes a running shell."""
import argparse, datetime, hashlib, json, math
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
RAW=ROOT/'docs/verify/raw/perf-shell'
def load(tag,kind):
    folder=tag+'-'+('live'if kind in ('live-idle','live-activity')else kind)
    suffix='activity'if kind=='live-activity'else'idle'
    return json.loads((RAW/folder/(folder+'-'+suffix+'.json')).read_text())
def native_cpu(data):
    p=data['shell']
    return p['task_counters']['runtime_ns']/1e9/data['seconds']*100 if not p['exited_task_count'] else p['cpu_percent']
def wrap(data):
    p=data['shell'];mode=data['agent_input_mode']
    return {**p,'cpu_percent':native_cpu(data),'process_tick_cpu_percent':p['cpu_percent'],'instrumented':data['instrumented'],'pid_namespace':'private'if'private PID'in mode else'host','agent_inputs_frozen':mode!='live hooks + host process discovery','hook_file_events':len(data['hook_events']),'host_hook_file_events':len(data['host_hook_events']),'non_scan_change_notifications':sum(e.get('non_scan_changes',0)>0 for e in data['hook_events']),'input_mode':mode}
def gpu_memory(data,side,family):
    endpoint=data[family+('_before'if side=='before'else'_after')]
    pids={int(p)for p in endpoint['processes']};rows=[r for r in data['gpu_'+side].get('rows',[])if r['pid']in pids]
    try:return str(sum(float(r['used_memory'].split()[0])for r in rows))if rows else'UNAVAILABLE'
    except (ValueError,TypeError):return'UNAVAILABLE'
def rss(data,family):
    p=data[family];samples=[sum(r['rss_kib']for r in s[family]['processes'].values())for s in data['samples']]
    return f"{p['rss_start_kib']/1024:.1f} → {p['rss_end_kib']/1024:.1f} (sampled max {max([p['rss_start_kib'],p['rss_end_kib'],*samples])/1024:.1f})"
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--tag',default='final');args=parser.parse_args();tag=args.tag
    quiet=load(tag,'quiet');no_hooks=load(tag,'no-hooks');idle=load(tag,'live-idle');active=load(tag,'live-activity')
    windows=[quiet,no_hooks,idle,active]
    if any(d['instrumented']or d['build_profile']!='release'or d['seconds']<60 for d in windows):raise SystemExit('Incomplete/debug/instrumented intervals cannot become headline results')
    if len({d['binary_sha256']for d in windows})!=1:raise SystemExit('Final windows used different binaries')
    binary=Path(idle['binary'])
    if hashlib.sha256(binary.read_bytes()).hexdigest()!=idle['binary_sha256']:raise SystemExit('Archived binary changed')
    source_path=RAW/(tag+'-live')/(tag+'-live-sources.json');sources=json.loads(source_path.read_text())
    summary={'schema':1,'mode':'real-shell-release','epoch':datetime.datetime.now(datetime.timezone.utc).timestamp(),'binary_path':str(binary),'binary_sha256':idle['binary_sha256'],'build_snapshot':sources,'live_idle':wrap(idle),'activity':wrap(active),'controlled_idle':wrap(quiet),'host_discovery_without_hooks':wrap(no_hooks),'raw_tag':tag,'provider_graph':'normal production assembly / Hub::spawn; --read-only suppresses actions and agent registration; no state JSON streaming'}
    (ROOT/'docs/verify/raw/perf-shell-latest.json').write_text(json.dumps(summary,indent=2)+'\n')
    status='NOT MET' if native_cpu(quiet)>.1 else 'controlled quiet PASS; full-host event-free acceptance remains separate'
    legacy_root=next((pid for pid,row in idle['legacy_before']['processes'].items() if row['comm'] in ('quickshell','qs')),None)
    lines=['# Real assembled shell performance','',f"Updated {datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')}. Selected run tag: `{tag}`.",'',
    f"**Idle budget: {status}.** the controlled zero-agent-input release session uses **{quiet['shell']['cpu_percent']:.4f}% process CPU**, **{native_cpu(quiet):.4f}% summed thread CPU**, versus ≤0.1%. Host process discovery with external hooks frozen uses {no_hooks['shell']['cpu_percent']:.4f}%. The actual live-hook idle interval uses {idle['shell']['cpu_percent']:.4f}%; its {len(idle['hook_events'])} hook-file notifications are reported separately, not blindly subtracted.",'',
    '## Setup and scope','',
    '- This is the actual `hyprv-shell` crate, release opt-level 3, fat LTO, one codegen unit, panic abort. It runs the normal production assembly, retained EGL/Skia UI and service graph. It is not the service probe, a rendering benchmark, `--headless` compatibility host, or a short fixture scene.',
    '- `--read-only --no-exclusive` exposes the existing assembly read-only parameter: no native control actions or network-agent registration. Session/system service buses stay live. Shell IPC, cache and state directories are private; agent hook files are observed without writing them. PipeWire uses the original runtime directory. The nested Hyprland service socket targets only the private instance.',
    '- A private D-Bus / virtual KWin parent hosts nested Hyprland. Native measured output and layers are verified at **6144×3456 physical, scale 2, 3072×1728 logical, 60 Hz**, matching the live DP-4 geometry. Only private KWin geometry is changed. A sacrificial read-only client can establish the output before measured clients start. No host compositor configuration, Quickshell UI, service or device is changed.',
    '- The shell uses its EGL path and appears as a graphics process on the NVIDIA RTX PRO 6000 Blackwell. NVIDIA per-process graphics allocations come from `nvidia-smi -q -x`; these are not total desktop GPU memory. The nested compositor may use software rendering and has no host blur/HDR rules; compositor processes are excluded from both application-family comparisons.',
    '- Native initialization/provider hydration is excluded (12-second warmup). Each headline interval is ≥60 seconds. Activity sends 12 successful UI-only IPC calls, alternating calendar and control-panel open/close every five seconds. Legacy Quickshell receives no injected activity; its same-window background observation is not an equal-workload activity benchmark.',
    ('- The live main Quickshell config (PID '+str(legacy_root)+') plus descendants is observed via procfs only. Family RSS can double-count shared pages.' if legacy_root else '- The host is already native. Its running shell supplies environment only and is never driven or restarted. No same-window legacy comparison is available; historical legacy figures are not presented as current measurements.'),'',
    '## Same-window CPU, scheduling and memory','',
    '| Window / family | CPU % process (thread runtime) | Sched / s | Context switches / s | RSS MiB start → end / sampled max | GPU MiB start → end |',
    '|---|---:|---:|---:|---|---|']
    for title,data in [('Controlled quiet (private PID namespace)',quiet),('Frozen hooks / host discovery',no_hooks),('Live-hook idle',idle),('Typical native UI activity',active)]:
        for family,label in [('shell','Rust'),('legacy','live Quickshell')]:
            if not data[family+'_before']['processes']:continue
            p=data[family];thread=p['task_counters']['runtime_ns']/1e9/data['seconds']*100
            lines.append(f"| {title} / {label} | {p['cpu_percent']:.4f} ({thread:.4f}) | {p['scheduling_proxy_per_s']:.2f} | {p['context_switches_per_s']:.2f} | {rss(data,family)} | {gpu_memory(data,'before',family)} → {gpu_memory(data,'after',family)} |")
    lines += ['',
    'Scheduling timeslices (`/proc/PID/task/TID/schedstat`) and voluntary/involuntary context switches are **wakeup proxies**, not exact `sched_wakeup` events. CPU process ticks include reaped child time; native thread runtime is the higher-resolution budget value when no threads exit. Endpoint task counters miss short-lived legacy children; exited/new task counts are retained in raw data. RSS is sampled about once per second; GPU memory is measured at endpoints only, not as a peak.',
    '', '## Agent events and eligibility','',
    '| Native interval | Native hook-file notifications | Notifications with observed non-ProcessScan session changes | Host hook notifications seen by observer | Agent inputs |',
    '|---|---:|---:|---:|---|']
    for title,data in [('Controlled quiet',quiet),('Host discovery without hooks',no_hooks),('Live idle',idle),('Activity',active)]:
        w=wrap(data);lines.append(f"| {title} | {w['hook_file_events']} | {w['non_scan_change_notifications']} | {w['host_hook_file_events']} | {w['input_mode']} |")
    lines += ['',
    'A file notification is not automatically a genuine tool/hook event: the legacy process scanner also rewrites state. The observer compares hashed session fields and classifies ProcessScan-only changes; coalesced file notifications can conceal intermediate states, so semantic counts are observations/lower bounds, not an exact event ledger. No session contents or tool arguments are copied into the event classification data.',
    '',
    'The controlled quiet session uses an empty agent-state fixture, absent app-server endpoint and a private PID namespace. Every service actor remains compiled and started, but process-dependent observations/discovery differ. The host-discovery session freezes only external hook inputs and retains real `/proc`; native process discovery and agent animation remain active. Neither isolated interval is silently accepted as a full-host idle pass. The controlled case alone cannot certify a full-host pass; current numeric status is shown above.',
    '', '## Historical investigation (not current-candidate causal proof)','',
    '1. **Fixed Shared-slot dirty feedback (`hyprv-ui-bar`).** `content_equal` lacked `Content::Shared`, so the same cached island `Rc<Node>` was unequal every reconciliation. Each paint scheduled another dirty frame. The comparator now uses `Rc::ptr_eq`; a changed tree still damages the slot. The regression fails before (one edit instead of zero), then all 17 bar behavior tests pass. [Before](raw/perf-shell/shared-slot-before.log), [after](raw/perf-shell/shared-slot-after.log).',
    '2. **Fixed clip canonicalization feedback (`hyprv-shell`).** The host compared a new unsnapped tree against the retained snapped tree and then finalized clips again. A 120.15625-wide center Rectangle alternated with a 120-wide Group plus a Rectangle child. A frozen diagnostic copy records the repeated mismatches. The host now canonicalizes before initial mounting and reconciliation. The regression fails before (four edits on an equal model), verifies unchanged updates stay clean and genuine stats still damage, and all 24 shell library tests pass. [Before](raw/perf-shell/clip-before.log), [after](raw/perf-shell/clip-after.log), [diagnostic trace](raw/perf-shell/damage-trace/shell-damage-trace-idle.log).',
    '3. **Historical periodic discovery (`hyprv-svc-desktop`).** Earlier candidates scanned host `/proc` every 1.5 seconds and renewed synthetic activity. The owner subsequently landed B2 process-discovery optimizations. This report measures the selected candidate afresh; it does not attribute its residual CPU to the former scan loop. Sequential frozen-hook and controlled windows are observations, not a causal subtraction. [Request](../requests/hyprv-svc-desktop.md).',
    '4. **Historical UI hot paths (`hyprv-shell` / `hyprv-ui-bar`).** Earlier controlled candidates spent most CPU in main-thread update/paint, including full bar reconstruction on callbacks and 1 Hz stats. Subsequent owners changed these paths. Current per-thread counters can localize CPU by thread, but this uninstrumented run does not separately measure tree construction, layout, painting or prove the previous diagnosis still applies. [Shell request](../requests/hyprv-shell.md), [bar request](../requests/hyprv-ui-bar.md).',
    '', '## Before / after integration observations','',
    '| Release case | CPU % process | Sched / s | Native hook events |', '|---|---:|---:|---:|']
    for title,path in [('Original controlled quiet',RAW/'release-quiet/release-quiet-idle.json'),('After Shared fix, before clip fix',RAW/'after-quiet/after-quiet-idle.json')]:
        if path.exists():
            d=json.loads(path.read_text());lines.append(f"| {title} | {d['shell']['cpu_percent']:.4f} | {d['shell']['scheduling_proxy_per_s']:.2f} | {len(d['hook_events'])} |")
    lines.append(f"| Selected final controlled quiet | {quiet['shell']['cpu_percent']:.4f} | {quiet['shell']['scheduling_proxy_per_s']:.2f} | {len(quiet['hook_events'])} |")
    lines += ['',
    'These whole-binary runs are sequential integration comparisons, not a one-line-only causal experiment: other agents changed renderer/shell code concurrently, and early runs lacked the PipeWire runtime override. Original preliminary runs hashed a mutable target path; final selected binaries are archived and immutable, and later refreshes hash the actual procfs executable as well. The final candidate is archived with frozen/source receipt fingerprints. The two failing-before/passing-after regressions isolate the actual dirty-feedback defects. Earlier 1280×720 setup trials and instrumented/debug/profile attempts are retained as diagnostics and excluded from headline results. Perf counting worked, but sampling-record permission was denied; no sysctls or privileges were changed.',
    '', '## Reproduction and provenance','',
    f"- Selected ELF: `{binary}`; SHA-256 `{idle['binary_sha256']}`.",
    f"- Inputs / equivalent release command: [source snapshot](raw/perf-shell/{tag}-live/{tag}-live-sources.json). Initial pinned build: [log](raw/perf-shell/frozen-release-build.log). Subsequent CI refreshes use a hash-verified staged production release.",
    '- Harness: `python3 -B docs/verify/measure-shell.py --label NAME` (live 60s + activity 60s); `--quiet-agents --idle-only` (controlled session); `--quiet-hooks --idle-only` (host discovery without external hook writes). `HYPRV_SHELL_BINARY` selects the archived real binary; source snapshots/receipts accompany it. The harness creates and stops only its own private processes.',
    f"- Raw selected runs: [quiet](raw/perf-shell/{tag}-quiet/{tag}-quiet-idle.json), [host discovery](raw/perf-shell/{tag}-no-hooks/{tag}-no-hooks-idle.json), [live idle](raw/perf-shell/{tag}-live/{tag}-live-idle.json), [activity](raw/perf-shell/{tag}-live/{tag}-live-activity.json).",
    '- Machine-readable real-shell gate input: [perf-shell-latest.json](raw/perf-shell-latest.json). Preflight now prefers this evidence over service-probe serialization measurements, validates archived ELF and UI/service input hashes, and rejects debug/instrumented/short or isolated evidence as a full-host pass. A controlled quiet failure also blocks the budget. [Gate regressions](raw/perf-shell/preflight-tests.log).',
    '', 'No install, cutover, host reload, legacy restart, privileged helper activation or device-control action was performed.']
    lines += ['', '## Hourly candidate history', '',
    '| Candidate tag / source snapshot | Quiet thread CPU % | Host discovery without hooks CPU % | Live-hook thread CPU % | Live hook-file notifications |',
    '|---|---:|---:|---:|---:|']
    for path in sorted(RAW.glob('ci-*-quiet/*-idle.json')):
        historical_tag=path.parent.name.removesuffix('-quiet')
        try:
            q=load(historical_tag,'quiet');h=load(historical_tag,'no-hooks');live=load(historical_tag,'live-idle');activity=load(historical_tag,'live-activity')
            if any(d['instrumented']or d['build_profile']!='release'or d['seconds']<60 for d in (q,h,live,activity)):continue
            if len({d['binary_sha256']for d in (q,h,live,activity)})!=1:continue
            source=json.loads((path.parent/(path.parent.name+'-sources.json')).read_text())
            fingerprint=source.get('ci_snapshot_fingerprint','not recorded')[:16]
            lines.append(f"| [{historical_tag}](raw/perf-shell/{path.parent.name}/{path.name}) / `{fingerprint}` | {native_cpu(q):.4f} | {native_cpu(h):.4f} | {native_cpu(live):.4f} | {len(live['hook_events'])} |")
        except (OSError,ValueError,KeyError):continue
    lines += ['', 'Only complete >=60s uninstrumented release windows from one ELF per candidate enter this history. Hook traffic is separate; differences between sequential windows are not exact event costs. Source snapshots are deleted after evidence is summarized under the disk policy; source hashes and command receipts remain. Only the selected measured ELF must remain available for the current performance gate.']
    (ROOT/'docs/verify/perf-shell.md').write_text('\n'.join(lines)+'\n')
    print('Updated perf-shell.md and perf-shell-latest.json:',tag,'quiet process',quiet['shell']['cpu_percent'],'quiet thread',native_cpu(quiet),flush=True)
if __name__=='__main__':main()
