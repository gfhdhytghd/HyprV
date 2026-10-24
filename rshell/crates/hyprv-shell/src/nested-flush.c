// Test-only workaround for Aquamarine versions that do not flush the initial
// xdg_surface commit before polling. Applied only to our private compositor.
#define _GNU_SOURCE
#include <dlfcn.h>
struct wl_display;
int wl_display_get_fd(struct wl_display *display) {
  typedef int (*display_fn)(struct wl_display *);
  static display_fn get_fd, flush;
  if (!get_fd)
    get_fd = (display_fn)dlsym(RTLD_NEXT, "wl_display_get_fd");
  if (!flush)
    flush = (display_fn)dlsym(RTLD_NEXT, "wl_display_flush");
  if (flush)
    flush(display);
  return get_fd ? get_fd(display) : -1;
}
