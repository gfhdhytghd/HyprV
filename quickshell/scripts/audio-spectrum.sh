#!/usr/bin/env bash

set -euo pipefail

if ! command -v cava >/dev/null 2>&1; then
    exit 127
fi

exec cava -p /dev/stdin <<'CAVACONF'
[general]
framerate=24
bars=16
autosens=1
sensitivity=45
lower_cutoff_freq=50
higher_cutoff_freq=12000

[output]
method=raw
raw_target=/dev/stdout
data_format=ascii
ascii_max_range=100
channels=mono
mono_option=average

[smoothing]
noise_reduction=35
integral=88
gravity=92
ignore=2
monstercat=1.4
CAVACONF
