#!/usr/bin/env python3
"""Read-only live /proc sampler and passive Hyprland event-name counter.

Scheduling slices/context switches are proxies, NOT sched_wakeup tracepoints.
Never records window titles or event payloads, and never sends compositor input.
"""
import argparse
import collections
import ctypes
import hashlib
import json
import pathlib
import os
import select
import socket
import struct
import time


def snapshot(pid):
    stat = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    threads = {}
    for task in pathlib.Path(f'/proc/{pid}/task').iterdir():
        try:
            status = dict(line.split(':', 1) for line in (task / 'status').read_text().splitlines() if ':' in line)
            runtime, delay, slices = map(int, (task / 'schedstat').read_text().split())
            threads[task.name] = dict(name=status['Name'].strip(), runtime_ns=runtime,
                delay_ns=delay, slices=slices,
                voluntary=int(status['voluntary_ctxt_switches']),
                involuntary=int(status['nonvoluntary_ctxt_switches']))
        except FileNotFoundError:
            pass
    return dict(monotonic=time.monotonic(), epoch=time.time(), threads=threads,
        process_ticks=int(stat[11]) + int(stat[12]), process_start_ticks=int(stat[19]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('output', type=pathlib.Path)
    parser.add_argument('--seconds', type=float, default=180)
    args = parser.parse_args()
    env = dict(row.split(b'=', 1) for row in pathlib.Path(f'/proc/{args.pid}/environ').read_bytes().split(b'\0') if b'=' in row)
    path = pathlib.Path(env[b'XDG_RUNTIME_DIR'].decode()) / 'hypr' / env[b'HYPRLAND_INSTANCE_SIGNATURE'].decode() / '.socket2.sock'
    executable = pathlib.Path(f'/proc/{args.pid}/exe')
    sha = hashlib.sha256(executable.read_bytes()).hexdigest()
    events = collections.Counter()
    hook_events = collections.Counter()
    libc = ctypes.CDLL(None, use_errno=True)
    hook_fd = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
    if hook_fd < 0:
        raise OSError(ctypes.get_errno(), 'inotify_init1')
    hook_path = pathlib.Path(env.get(b'HYPRV_AGENT_ISLAND_DIR', os.fsencode(pathlib.Path.home() / '.cache/hyprv/agent-island')).decode())
    wd = libc.inotify_add_watch(hook_fd, os.fsencode(hook_path), 0x8 | 0x80 | 0x100 | 0x200)
    if wd < 0:
        os.close(hook_fd)
        raise OSError(ctypes.get_errno(), 'inotify_add_watch')
    with socket.socket(socket.AF_UNIX) as stream:
        stream.connect(str(path))
        before = snapshot(args.pid)
        samples = [before]
        end = time.monotonic() + args.seconds
        next_sample = time.monotonic() + 1
        pending = b''
        while time.monotonic() < end:
            ready, _, _ = select.select([stream, hook_fd], [], [], max(0, min(end, next_sample) - time.monotonic()))
            if hook_fd in ready:
                data = os.read(hook_fd, 65536)
                offset = 0
                while offset + 16 <= len(data):
                    _, mask, _, length = struct.unpack_from('iIII', data, offset)
                    name = data[offset + 16:offset + 16 + length].split(b'\0', 1)[0]
                    offset += 16 + length
                    if name == b'state.json':
                        hook_events[str(mask)] += 1
            if stream in ready:
                data = stream.recv(65536)
                if not data:
                    raise RuntimeError('compositor event socket closed')
                pending += data
                while b'\n' in pending:
                    row, pending = pending.split(b'\n', 1)
                    events[row.split(b'>>', 1)[0].decode(errors='replace')] += 1
            if time.monotonic() >= next_sample:
                samples.append(snapshot(args.pid))
                next_sample += 1
        after = snapshot(args.pid)
    os.close(hook_fd)
    seconds = after['monotonic'] - before['monotonic']
    if after['process_start_ticks'] != before['process_start_ticks']:
        raise RuntimeError('PID identity changed during measurement')
    delta = {}
    for tid, row in after['threads'].items():
        if tid in before['threads']:
            first = before['threads'][tid]
            delta[tid] = {key: row[key] - first[key] for key in row if key != 'name'}
            delta[tid].update(name=row['name'], cpu_percent=delta[tid]['runtime_ns'] / seconds / 1e7,
                voluntary_per_s=delta[tid]['voluntary'] / seconds, slices_per_s=delta[tid]['slices'] / seconds)
    last_seen = {}
    for sample in [before, *samples, after]:
        last_seen.update(sample['threads'])
    accounted = {}
    for tid, row in last_seen.items():
        first = before['threads'].get(tid, {})
        accounted[tid] = dict(name=row['name'],
            cpu_percent=(row['runtime_ns'] - first.get('runtime_ns', 0)) / seconds / 1e7,
            voluntary_per_s=(row['voluntary'] - first.get('voluntary', 0)) / seconds,
            slices_per_s=(row['slices'] - first.get('slices', 0)) / seconds,
            present_before=tid in before['threads'], present_after=tid in after['threads'])
    result = dict(pid=args.pid, binary_sha256=sha, seconds=seconds, threads=delta,
        cpu_percent=sum(row['cpu_percent'] for row in delta.values()),
        process_cpu_percent=(after['process_ticks'] - before['process_ticks']) / os.sysconf('SC_CLK_TCK') / seconds * 100,
        process_cpu_quantum_percent=100 / os.sysconf('SC_CLK_TCK') / seconds,
        sampled_threads=accounted,
        sampled_thread_cpu_lower_bound_percent=sum(row['cpu_percent'] for row in accounted.values()),
        socket2_events=dict(events), hook_file_events=dict(hook_events), before=before, after=after, samples=samples,
        boundary='Read-only observation; context switches/slices are not exact wakeup trace events. cpu_percent is matching endpoint threads only. process_cpu_percent includes exited threads at process-tick resolution. sampled_threads includes last-observed runtime of exited/new threads but can miss work between samples.')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key not in ('samples', 'before', 'after')}, indent=2))


if __name__ == '__main__':
    main()
