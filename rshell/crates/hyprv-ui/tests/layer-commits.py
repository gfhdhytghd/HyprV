#!/usr/bin/env python3
"""Check atomic layer resizes in a WAYLAND_DEBUG=client trace (no live IPC)."""
import argparse
import json
import hashlib
import os
import re
from pathlib import Path
import subprocess
import unittest


def check(trace):
    surfaces, layers, viewports = {}, {}, {}
    errors, resizes, grows, shrinks, commits = [], 0, 0, 0, 0
    request = re.compile(r'-> (\w+)[#@](\d+)\.(\w+)\((.*)\)')
    for number, line in enumerate(trace.splitlines(), 1):
        match = request.search(line)
        if not match:
            continue
        interface, object_id, method, args = match.groups()
        ids = re.findall(r'(\w+)[#@](\d+)', args)
        if interface == 'zwlr_layer_shell_v1' and method == 'get_layer_surface':
            layer, surface = ids[0][1], ids[1][1]
            layers[layer] = surface
            surfaces[surface] = dict(size=None, pending_size=None, buffer=None,
                                     attach=False, destination=None, pending_destination=None)
        elif interface == 'wp_viewporter' and method == 'get_viewport':
            viewports[ids[0][1]] = ids[1][1]
        elif interface == 'zwlr_layer_surface_v1' and method == 'set_size':
            surfaces[layers[object_id]]['pending_size'] = tuple(map(int, args.split(', ')))
        elif interface == 'wp_viewport' and method == 'set_destination':
            surface = viewports.get(object_id)
            if surface in surfaces:
                surfaces[surface]['pending_destination'] = tuple(map(int, args.split(', ')))
        elif interface == 'wl_surface' and object_id in surfaces:
            state = surfaces[object_id]
            if method == 'attach':
                state['attach'] = True
                state['next_buffer'] = ids[0][1] if ids else None
            elif method == 'destroy':
                del surfaces[object_id]
            elif method == 'commit':
                commits += 1
                size = state['pending_size']
                destination = state['pending_destination'] or state['destination']
                buffer = state.get('next_buffer') if state['attach'] else state['buffer']
                if size is not None and state['buffer'] is not None and size != state['size']:
                    resizes += 1
                    previous = state['size']
                    if previous and size[1] and previous[1]:
                        grows += size[1] > previous[1]
                        shrinks += size[1] < previous[1]
                    if not state['attach'] or buffer is None:
                        errors.append(f'line {number}: surface {object_id} resized {state["size"]}->{size} without a replacement buffer')
                if size is not None and buffer is not None:
                    if destination is None:
                        errors.append(f'line {number}: cannot verify buffer logical extent without viewport destination')
                    elif any(want and want != actual for want, actual in zip(size, destination)):
                        errors.append(f'line {number}: layer size {size} mismatches buffer destination {destination}')
                if size is not None:
                    state['size'] = size
                state.update(pending_size=None, buffer=buffer, attach=False,
                             destination=destination, pending_destination=None)
    return dict(errors=errors, resizes=resizes, grows=grows, shrinks=shrinks, commits=commits)


PREFIX = '''-> zwlr_layer_shell_v1#1.get_layer_surface(new id zwlr_layer_surface_v1#3, wl_surface#2, nil, 2, "test")
-> wp_viewporter#4.get_viewport(new id wp_viewport#5, wl_surface#2)
-> zwlr_layer_surface_v1#3.set_size(300, 820)
-> wl_surface#2.commit()
-> wp_viewport#5.set_destination(300, 820)
-> wl_surface#2.attach(wl_buffer#6, 0, 0)
-> wl_surface#2.commit()
'''


class Regression(unittest.TestCase):
    def test_old_buffer_resize_commit_is_rejected(self):
        trace = PREFIX + '-> zwlr_layer_surface_v1#3.set_size(300, 58)\n-> wl_surface#2.commit()'
        self.assertTrue(check(trace)['errors'])

    def test_new_buffer_with_stale_extent_is_rejected(self):
        trace = PREFIX + '''-> zwlr_layer_surface_v1#3.set_size(300, 58)
-> wl_surface#2.attach(wl_buffer#7, 0, 0)
-> wl_surface#2.commit()'''
        self.assertTrue(check(trace)['errors'])

    def test_atomic_resize_and_metadata_only_commits_are_valid(self):
        trace = PREFIX + '''-> wl_surface#2.set_input_region(wl_region#8)
-> wl_surface#2.commit()
-> wl_surface#2.frame(new id wl_callback#9)
-> wl_surface#2.commit()
-> zwlr_layer_surface_v1#3.set_size(300, 58)
-> wp_viewport#5.set_destination(300, 58)
-> wl_surface#2.attach(wl_buffer#7, 0, 0)
-> wl_surface#2.commit()'''
        result = check(trace)
        self.assertEqual(result['errors'], [])
        self.assertEqual(result['shrinks'], 1)

    def test_stretched_axis_still_requires_atomic_explicit_axis(self):
        trace = PREFIX + '''-> zwlr_layer_surface_v1#3.set_size(0, 58)
-> wl_surface#2.commit()'''
        self.assertTrue(check(trace)['errors'])

    def test_zero_axis_does_not_mean_zero_buffer(self):
        trace = PREFIX + '''-> zwlr_layer_surface_v1#3.set_size(0, 58)
-> wp_viewport#5.set_destination(300, 58)
-> wl_surface#2.attach(wl_buffer#7, 0, 0)
-> wl_surface#2.commit()'''
        self.assertEqual(check(trace)['errors'], [])


def run_nested(binary, output):
    root = Path(__file__).resolve().parents[3]
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    pinned = output / 'hyprv-shell'
    os.link(binary.resolve(), pinned)
    receipt = dict(binary_sha256=hashlib.sha256(pinned.read_bytes()).hexdigest(), runs=[])
    try:
        for name, profile, probe in [('desktop-probe', 'desktop', True),
                                     ('laptop-probe', 'laptop', True),
                                     ('desktop-components', 'desktop', False)]:
            env = os.environ.copy()
            for key in ('HYPRV_FUNCTIONAL_CALLBACKS_ONLY', 'HYPRV_IDLE_TEST',
                        'HYPRV_FUNCTIONAL_LAYER_COMMITS_ONLY', 'HYPRV_CALLBACK_NEGATIVE_CONTROL'):
                env.pop(key, None)
            env.update(HYPRV_PROFILE=profile, WAYLAND_DEBUG='client',
                       HYPRV_FUNCTIONAL_FILTER='engine.layer-resize' if probe else 'calendar,island.music-expand-collapse,bar.toggle')
            if probe:
                env['HYPRV_FUNCTIONAL_LAYER_COMMITS_ONLY'] = '1'
            destination = output / name
            with (output / (name + '.log')).open('w') as log:
                subprocess.run([str(pinned), '--nested-functional-test', str(destination)],
                               cwd=root, env=env, stdout=log, stderr=log, timeout=180, check=True)
            manifest = json.loads((destination / 'manifest.json').read_text())
            if not manifest['pass']:
                raise RuntimeError(f'{name}: functional fixture failed')
            if probe and not any(case['id'] == 'engine.layer-resize' and case['status'] == 'PASS'
                                 for case in manifest['cases']):
                raise RuntimeError(f'{name}: missing resize probe')
            traces = list(destination.glob('run-*/fixture/shell.err'))
            if len(traces) != 1:
                raise RuntimeError(f'{name}: expected one complete client trace')
            result = check(traces[0].read_text())
            if result['errors'] or not result['grows'] or not result['shrinks']:
                raise RuntimeError(f'{name}: {result}')
            if probe and result['resizes'] != 5:
                raise RuntimeError(f'{name}: expected all five mapped probe transitions: {result}')
            receipt['runs'].append(dict(name=name, **result))
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt, indent=2))
    finally:
        pinned.unlink()
        for artifact in output.glob('*/hyprv-shell-test-*'):
            artifact.unlink()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', nargs='?', type=Path)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.binary:
        if not args.output or args.trace:
            parser.error('--binary needs --output and no trace argument')
        run_nested(args.binary, args.output)
    elif args.trace is None:
        unittest.main(argv=[__file__])
    else:
        result = check(args.trace.read_text())
        print(json.dumps(result, indent=2))
        if result['errors'] or not result['grows'] or not result['shrinks']:
            raise SystemExit('FAIL: need both grow/shrink coverage and zero mismatched commits')
