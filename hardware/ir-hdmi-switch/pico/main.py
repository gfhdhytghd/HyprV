"""USB-serial IR envelope capture firmware for an RP2040 Pico.

The receiver output is expected to be active-low.  Commands and responses are
newline-delimited JSON so the host can store captures without losing timing.
"""

import micropython
import select
import sys
import time
import ujson
from machine import PWM, Pin, time_pulse_us


FIRMWARE_VERSION = 2
DEFAULT_PIN = 0
PIN_FILE = "ir_pin.txt"
VALID_PINS = tuple(range(23)) + (26, 27, 28)
MAX_DURATIONS = 512

micropython.alloc_emergency_exception_buf(100)


def emit(event, **values):
    values["event"] = event
    print(ujson.dumps(values))


def load_pin():
    try:
        with open(PIN_FILE, "r") as handle:
            value = int(handle.read().strip())
            if value in VALID_PINS:
                return value
    except (OSError, ValueError):
        pass
    return DEFAULT_PIN


current_pin = load_pin()


def set_pin(value):
    global current_pin
    value = int(value)
    if value not in VALID_PINS:
        raise ValueError("pin must be GP0-GP22 or GP26-GP28")
    current_pin = value
    with open(PIN_FILE, "w") as handle:
        handle.write(str(value))
    return value


def scan_pins(duration_ms=5000):
    """Count edges on pulled-up GPIOs while the user presses a remote key."""
    counts = [0] * len(VALID_PINS)
    pins = []
    handlers = []

    for index, number in enumerate(VALID_PINS):
        pin = Pin(number, Pin.IN, Pin.PULL_UP)

        def handler(_pin, index=index):
            counts[index] += 1

        pins.append(pin)
        handlers.append(handler)
        pin.irq(handler=handler, trigger=Pin.IRQ_RISING | Pin.IRQ_FALLING)

    deadline = time.ticks_add(time.ticks_ms(), duration_ms)
    while time.ticks_diff(deadline, time.ticks_ms()) > 0:
        time.sleep_ms(20)

    for pin in pins:
        pin.irq(handler=None)

    candidates = []
    for index, count in enumerate(counts):
        if count >= 8:
            candidates.append({"pin": VALID_PINS[index], "edges": count})
    candidates.sort(key=lambda item: item["edges"], reverse=True)
    return candidates


def capture(pin_number, wait_ms=10000, gap_us=40000):
    """Capture one complete active-low demodulated IR frame."""
    pin = Pin(pin_number, Pin.IN, Pin.PULL_UP)
    first_mark = time_pulse_us(pin, 0, wait_ms * 1000)
    if first_mark < 0:
        return None

    durations = [first_mark]
    level = 1
    while len(durations) < MAX_DURATIONS:
        duration = time_pulse_us(pin, level, gap_us)
        if duration < 0:
            break
        # A long inactive space separates a full command from a repeat frame.
        if level == 1 and duration > 10000:
            break
        durations.append(duration)
        level ^= 1
    return durations


def transmit(pin_number, durations, carrier_hz=38000, duty_cycle=0.33):
    """Replay alternating carrier marks and silent spaces once."""
    if not durations or len(durations) > MAX_DURATIONS:
        raise ValueError("invalid duration count")
    if carrier_hz < 20000 or carrier_hz > 60000:
        raise ValueError("carrier must be 20000-60000 Hz")
    if duty_cycle <= 0 or duty_cycle > 0.5:
        raise ValueError("duty cycle must be >0 and <=0.5")

    pin = Pin(pin_number, Pin.OUT, value=0)
    pwm = PWM(pin)
    pwm.freq(carrier_hz)
    mark_duty = int(65535 * duty_cycle)
    pwm.duty_u16(0)
    try:
        for index, duration in enumerate(durations):
            if duration < 50 or duration > 100000:
                raise ValueError("duration must be 50-100000 us")
            pwm.duty_u16(mark_duty if index % 2 == 0 else 0)
            time.sleep_us(duration)
    finally:
        pwm.duty_u16(0)
        pwm.deinit()
        Pin(pin_number, Pin.OUT, value=0)


def handle(line):
    parts = line.strip().split()
    if not parts:
        return
    command = parts[0].lower()

    if command == "status":
        emit(
            "status",
            firmware=FIRMWARE_VERSION,
            pin=current_pin,
            polarity="active_low",
        )
    elif command == "pin" and len(parts) == 2:
        emit("pin", pin=set_pin(parts[1]))
    elif command == "scan":
        duration_ms = int(parts[1]) if len(parts) == 2 else 5000
        emit("scan_started", duration_ms=duration_ms)
        emit("scan", candidates=scan_pins(duration_ms))
    elif command == "capture":
        wait_ms = int(parts[1]) if len(parts) == 2 else 10000
        emit("capture_started", pin=current_pin, wait_ms=wait_ms)
        durations = capture(current_pin, wait_ms)
        if durations is None:
            emit("capture_timeout", pin=current_pin)
        else:
            emit(
                "capture",
                pin=current_pin,
                polarity="active_low",
                durations_us=durations,
            )
    elif command == "transmit" and len(parts) == 4:
        carrier_hz = int(parts[1])
        duty_cycle = int(parts[2]) / 10000
        durations = [int(value) for value in parts[3].split(",")]
        transmit(current_pin, durations, carrier_hz, duty_cycle)
        emit(
            "transmit",
            pin=current_pin,
            carrier_hz=carrier_hz,
            duty_cycle=duty_cycle,
            duration_count=len(durations),
        )
    elif command == "help":
        emit(
            "help",
            commands=[
                "status",
                "pin N",
                "scan [ms]",
                "capture [ms]",
                "transmit HZ DUTY_X10000 DURATIONS_CSV",
            ],
        )
    else:
        emit("error", message="unknown command", command=line.strip())


emit("ready", firmware=FIRMWARE_VERSION, pin=current_pin)
poll = select.poll()
poll.register(sys.stdin, select.POLLIN)

while True:
    if poll.poll(100):
        try:
            handle(sys.stdin.readline())
        except Exception as error:
            emit("error", message=str(error))
