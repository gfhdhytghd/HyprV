// titlewin — a fully transparent xdg toplevel with a given title, so the nested Hyprland
// has an active window (Quickshell.Hyprland activeToplevel) without painting anything.
// usage: titlewin TITLE   (WAYLAND_DISPLAY = the nested instance; runs until killed)
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include "xdg-shell-client-protocol.h"

static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct xdg_wm_base *wm_base;
static struct wl_surface *surface;
static int width = 64, height = 64, configured;

static void ping(void *d, struct xdg_wm_base *b, uint32_t serial) { (void)d; xdg_wm_base_pong(b, serial); }
static const struct xdg_wm_base_listener wm_listener = {ping};

static void global(void *d, struct wl_registry *r, uint32_t name, const char *iface, uint32_t ver)
{
    (void)d; (void)ver;
    if (!strcmp(iface, wl_compositor_interface.name))
        compositor = wl_registry_bind(r, name, &wl_compositor_interface, 4);
    else if (!strcmp(iface, wl_shm_interface.name))
        shm = wl_registry_bind(r, name, &wl_shm_interface, 1);
    else if (!strcmp(iface, xdg_wm_base_interface.name)) {
        wm_base = wl_registry_bind(r, name, &xdg_wm_base_interface, 1);
        xdg_wm_base_add_listener(wm_base, &wm_listener, NULL);
    }
}
static void global_remove(void *d, struct wl_registry *r, uint32_t n) { (void)d; (void)r; (void)n; }
static const struct wl_registry_listener reg_listener = {global, global_remove};

static void attach_transparent(void)
{
    const int stride = width * 4, size = stride * height;
    int fd = memfd_create("titlewin", MFD_CLOEXEC);
    if (fd < 0 || ftruncate(fd, size) < 0)
        exit(1);
    struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, size); // zero-filled: alpha 0
    struct wl_buffer *buf = wl_shm_pool_create_buffer(pool, 0, width, height, stride, WL_SHM_FORMAT_ARGB8888);
    wl_shm_pool_destroy(pool);
    close(fd);
    wl_surface_attach(surface, buf, 0, 0);
    wl_surface_damage(surface, 0, 0, width, height);
    wl_surface_commit(surface);
}

static void surface_configure(void *d, struct xdg_surface *s, uint32_t serial)
{
    (void)d;
    xdg_surface_ack_configure(s, serial);
    attach_transparent();
    configured = 1;
}
static const struct xdg_surface_listener surface_listener = {surface_configure};

static void top_configure(void *d, struct xdg_toplevel *t, int32_t w, int32_t h, struct wl_array *st)
{
    (void)d; (void)t; (void)st;
    if (w > 0 && h > 0) {
        width = w;
        height = h;
    }
}
static void top_close(void *d, struct xdg_toplevel *t) { (void)d; (void)t; }
static const struct xdg_toplevel_listener top_listener = {top_configure, top_close, NULL, NULL};

int main(int argc, char **argv)
{
    struct wl_display *dpy = wl_display_connect(NULL);
    if (!dpy || argc < 2)
        return 2;
    struct wl_registry *reg = wl_display_get_registry(dpy);
    wl_registry_add_listener(reg, &reg_listener, NULL);
    wl_display_roundtrip(dpy);
    if (!compositor || !shm || !wm_base)
        return 3;
    surface = wl_compositor_create_surface(compositor);
    struct xdg_surface *xs = xdg_wm_base_get_xdg_surface(wm_base, surface);
    xdg_surface_add_listener(xs, &surface_listener, NULL);
    struct xdg_toplevel *top = xdg_surface_get_toplevel(xs);
    xdg_toplevel_add_listener(top, &top_listener, NULL);
    xdg_toplevel_set_title(top, argv[1]);
    xdg_toplevel_set_app_id(top, "parity-titlewin");
    wl_surface_commit(surface);
    while (wl_display_dispatch(dpy) != -1) {
        if (configured == 1) {
            printf("mapped\n");
            fflush(stdout);
            configured = 2;
        }
    }
    return 0;
}
