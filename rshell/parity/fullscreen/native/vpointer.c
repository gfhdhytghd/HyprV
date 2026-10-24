// vpointer — drive the pointer of the NESTED compositor (WAYLAND_DISPLAY must be the nested
// instance; the harness sets it). Commands on stdin, one per line; replies "ok\n" after the
// compositor has processed each command (wl_display_roundtrip):
//   move X Y W H     absolute position X,Y in an output-local box of W×H logical px
//   down BUTTON      BUTTON = left | right | middle
//   up BUTTON
//   axis DY          vertical wheel, DY in Qt angleDelta units (120 = one notch, + = up)
#include <linux/input-event-codes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

static struct wl_seat *seat;
static struct wl_output *output;
static struct zwlr_virtual_pointer_manager_v1 *manager;

static void global(void *data, struct wl_registry *reg, uint32_t name, const char *iface, uint32_t ver)
{
    (void)data;
    if (!strcmp(iface, wl_seat_interface.name) && !seat)
        seat = wl_registry_bind(reg, name, &wl_seat_interface, 1);
    else if (!strcmp(iface, wl_output_interface.name) && !output)
        output = wl_registry_bind(reg, name, &wl_output_interface, 1);
    else if (!strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name))
        manager = wl_registry_bind(reg, name, &zwlr_virtual_pointer_manager_v1_interface, ver < 2 ? ver : 2);
}
static void global_remove(void *data, struct wl_registry *reg, uint32_t name) { (void)data; (void)reg; (void)name; }
static const struct wl_registry_listener reg_listener = {global, global_remove};

static uint32_t now_ms(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static uint32_t button_code(const char *b)
{
    if (!strcmp(b, "right"))
        return BTN_RIGHT;
    if (!strcmp(b, "middle"))
        return BTN_MIDDLE;
    return BTN_LEFT;
}

int main(void)
{
    struct wl_display *dpy = wl_display_connect(NULL);
    if (!dpy) {
        fprintf(stderr, "vpointer: cannot connect to WAYLAND_DISPLAY\n");
        return 2;
    }
    struct wl_registry *reg = wl_display_get_registry(dpy);
    wl_registry_add_listener(reg, &reg_listener, NULL);
    wl_display_roundtrip(dpy);
    if (!manager) {
        fprintf(stderr, "vpointer: compositor lacks zwlr_virtual_pointer_manager_v1\n");
        return 3;
    }
    struct zwlr_virtual_pointer_v1 *ptr =
        wl_proxy_get_version((struct wl_proxy *)manager) >= 2 && output
            ? zwlr_virtual_pointer_manager_v1_create_virtual_pointer_with_output(manager, seat, output)
            : zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, seat);
    wl_display_roundtrip(dpy);

    char line[256];
    setvbuf(stdout, NULL, _IOLBF, 0);
    while (fgets(line, sizeof line, stdin)) {
        double x, y, w, h, dy;
        char b[16];
        if (sscanf(line, "move %lf %lf %lf %lf", &x, &y, &w, &h) == 4) {
            // Sub-pixel precision: extents in 1/100 logical px.
            zwlr_virtual_pointer_v1_motion_absolute(ptr, now_ms(), (uint32_t)(x * 100 + 0.5),
                                                    (uint32_t)(y * 100 + 0.5), (uint32_t)(w * 100),
                                                    (uint32_t)(h * 100));
            zwlr_virtual_pointer_v1_frame(ptr);
        } else if (sscanf(line, "down %15s", b) == 1) {
            zwlr_virtual_pointer_v1_button(ptr, now_ms(), button_code(b), WL_POINTER_BUTTON_STATE_PRESSED);
            zwlr_virtual_pointer_v1_frame(ptr);
        } else if (sscanf(line, "up %15s", b) == 1) {
            zwlr_virtual_pointer_v1_button(ptr, now_ms(), button_code(b), WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(ptr);
        } else if (sscanf(line, "axis %lf", &dy) == 1) {
            // Qt: angleDelta.y = −value·8 for wheel; one notch = value 15, discrete 1.
            const double value = -dy / 8.0;
            zwlr_virtual_pointer_v1_axis_source(ptr, WL_POINTER_AXIS_SOURCE_WHEEL);
            zwlr_virtual_pointer_v1_axis_discrete(ptr, now_ms(), WL_POINTER_AXIS_VERTICAL_SCROLL,
                                                  wl_fixed_from_double(value), (int)(-dy / 120));
            zwlr_virtual_pointer_v1_frame(ptr);
        } else {
            printf("error %s", line);
            continue;
        }
        wl_display_roundtrip(dpy);
        printf("ok\n");
    }
    zwlr_virtual_pointer_v1_destroy(ptr);
    wl_display_disconnect(dpy);
    return 0;
}
