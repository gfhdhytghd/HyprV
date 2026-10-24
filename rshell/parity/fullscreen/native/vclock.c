// vclock.so — virtual time for the ORIGINAL quickshell running in a nested compositor.
//
// LD_PRELOAD'ed into the shell (and inherited by its child processes). Time only moves when
// the harness writes a new offset, so a capture taken at virtual time T shows exactly the
// state at T, independent of real scheduling — the nested-compositor equivalent of
// parity-host's QAnimationDriver.
//
//   VCLOCK_FILE=<path>          8-byte file: little-endian int64 virtual offset in µs
//   VCLOCK_REALTIME_MS=<epoch>  wall-clock time at offset 0 (fixture "now", ms since epoch)
//
// CLOCK_MONOTONIC* / CLOCK_BOOTTIME → (real monotonic at first use) + offset
// CLOCK_REALTIME*                   → VCLOCK_REALTIME_MS + offset; gettimeofday/time likewise.
// Other clocks (process/thread CPU time) pass through.
//
// While active, finite poll/ppoll/epoll_wait timeouts are capped at VCLOCK_POLL_CAP_MS
// (default 4): an event loop that computed "sleep 5 s until the next timer" from the frozen
// clock still notices promptly when the harness advances time. Infinite waits (-1) are left
// alone: no timer is pending then, and libwayland asserts that poll(-1) never returns 0.
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <poll.h>
#include <signal.h>
#include <sys/epoll.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

typedef int (*clock_gettime_fn)(clockid_t, struct timespec *);

static clock_gettime_fn real_clock_gettime;
static volatile const int64_t *offset_us; // mmap of VCLOCK_FILE (NULL = inactive)
static int64_t mono_base_ns = -1;
static int64_t realtime_base_ns;
static int poll_cap_ms = 4;

static void init(void)
{
    if (real_clock_gettime)
        return;
    real_clock_gettime = (clock_gettime_fn)dlsym(RTLD_NEXT, "clock_gettime");
    const char *file = getenv("VCLOCK_FILE");
    const char *rt = getenv("VCLOCK_REALTIME_MS");
    if (!file || !rt)
        return;
    int fd = open(file, O_RDONLY | O_CLOEXEC);
    if (fd < 0)
        return;
    void *p = mmap(NULL, sizeof(int64_t), PROT_READ, MAP_SHARED, fd, 0);
    close(fd);
    if (p == MAP_FAILED)
        return;
    realtime_base_ns = (int64_t)strtoll(rt, NULL, 10) * 1000000LL;
    struct timespec ts;
    real_clock_gettime(CLOCK_MONOTONIC, &ts);
    mono_base_ns = (int64_t)ts.tv_sec * 1000000000LL + ts.tv_nsec;
    const char *cap = getenv("VCLOCK_POLL_CAP_MS");
    if (cap)
        poll_cap_ms = atoi(cap);
    offset_us = (volatile const int64_t *)p;
}

static void set_ns(struct timespec *ts, int64_t ns)
{
    ts->tv_sec = ns / 1000000000LL;
    ts->tv_nsec = ns % 1000000000LL;
}

int clock_gettime(clockid_t id, struct timespec *ts)
{
    init();
    if (!offset_us)
        return real_clock_gettime(id, ts);
    const int64_t off_ns = *offset_us * 1000LL;
    switch (id) {
    case CLOCK_MONOTONIC:
    case CLOCK_MONOTONIC_RAW:
    case CLOCK_MONOTONIC_COARSE:
    case CLOCK_BOOTTIME:
        set_ns(ts, mono_base_ns + off_ns);
        return 0;
    case CLOCK_REALTIME:
    case CLOCK_REALTIME_COARSE:
        set_ns(ts, realtime_base_ns + off_ns);
        return 0;
    default:
        return real_clock_gettime(id, ts);
    }
}

int gettimeofday(struct timeval *tv, void *tz)
{
    (void)tz;
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    tv->tv_sec = ts.tv_sec;
    tv->tv_usec = ts.tv_nsec / 1000;
    return 0;
}

time_t time(time_t *out)
{
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    if (out)
        *out = ts.tv_sec;
    return ts.tv_sec;
}

static int cap_ms(int timeout)
{
    init();
    if (!offset_us || poll_cap_ms <= 0)
        return timeout;
    return timeout > poll_cap_ms ? poll_cap_ms : timeout;
}

int poll(struct pollfd *fds, nfds_t n, int timeout)
{
    static int (*real)(struct pollfd *, nfds_t, int);
    if (!real)
        real = (int (*)(struct pollfd *, nfds_t, int))dlsym(RTLD_NEXT, "poll");
    return real(fds, n, cap_ms(timeout));
}

int ppoll(struct pollfd *fds, nfds_t n, const struct timespec *tmo, const sigset_t *mask)
{
    static int (*real)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *);
    if (!real)
        real = (int (*)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *))dlsym(RTLD_NEXT, "ppoll");
    const long long ms = tmo ? tmo->tv_sec * 1000LL + tmo->tv_nsec / 1000000 : -1;
    const int capped = cap_ms(ms > 1000000000LL ? 1000000000 : (int)ms);
    if (!tmo || capped >= ms)
        return real(fds, n, tmo, mask);
    const struct timespec c = {capped / 1000, (long)(capped % 1000) * 1000000L};
    return real(fds, n, &c, mask);
}

int epoll_wait(int fd, struct epoll_event *ev, int max, int timeout)
{
    static int (*real)(int, struct epoll_event *, int, int);
    if (!real)
        real = (int (*)(int, struct epoll_event *, int, int))dlsym(RTLD_NEXT, "epoll_wait");
    return real(fd, ev, max, cap_ms(timeout));
}

int epoll_pwait(int fd, struct epoll_event *ev, int max, int timeout, const sigset_t *mask)
{
    static int (*real)(int, struct epoll_event *, int, int, const sigset_t *);
    if (!real)
        real = (int (*)(int, struct epoll_event *, int, int, const sigset_t *))dlsym(RTLD_NEXT, "epoll_pwait");
    return real(fd, ev, max, cap_ms(timeout), mask);
}
