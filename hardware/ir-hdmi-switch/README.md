# Pico HDMI switch IR learner

This directory captures the HDMI switch remote's demodulated IR envelope. It
keeps the raw mark/space timings as the source of truth and adds NEC decoding
when the frame matches NEC.

## Receiver wiring

- Receiver `VCC` -> Pico `3V3(OUT)` (physical pin 36)
- Receiver `GND` -> Pico `GND`
- Receiver `OUT` -> `GP0` (physical pin 1)

Pico GPIO is not 5 V tolerant. Do not feed a 5 V receiver output directly into
it. The firmware expects the usual demodulating receiver's active-low output,
not a bare IR photodiode.

## Learn the remote

Run the Quickshell helper from the HyprV configuration root:

```sh
./quickshell/scripts/hdmi-switch-ir.py status
./quickshell/scripts/hdmi-switch-ir.py scan
./quickshell/scripts/hdmi-switch-ir.py set-pin 0
./quickshell/scripts/hdmi-switch-ir.py learn input1
./quickshell/scripts/hdmi-switch-ir.py learn input2
./quickshell/scripts/hdmi-switch-ir.py list
```

During `scan`, hold a remote button for five seconds. The GPIO with many edges
is the receiver `OUT` pin. Set it once with `set-pin`; it persists on the Pico.

Captures are stored at
`~/.local/share/hyprv-ir/hdmi-switch.json`. The file also reserves the assumed
38 kHz carrier and 33% duty cycle for the transmitter stage. Those values can
be adjusted after the captured protocol is known.

## Transmit a learned button

Connect a transistor-driven IR transmitter module with `SIG` on `GP0`. The
firmware emits an active-high 38 kHz carrier and leaves GP0 low while idle.
Pico GPIO must not drive a high-current IR LED directly.

```sh
./quickshell/scripts/hdmi-switch-ir.py send input1
```

Each command replays the learned frame exactly once.

## Pico installation

The Pico runs the official MicroPython RPI_PICO UF2. Copy `pico/main.py` to the
device as `main.py` after flashing:

```sh
uvx --from mpremote mpremote cp pico/main.py :main.py
uvx --from mpremote mpremote reset
```
