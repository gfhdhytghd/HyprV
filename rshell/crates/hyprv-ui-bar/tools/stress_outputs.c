/* Offline private-Weston module: real concurrent 60/144Hz headless outputs.
 * Built only by stress_lifecycle.py into its temporary private runtime. */
#include <libweston/libweston.h>
#include <libweston/windowed-output-api.h>
#include <wayland-server-core.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
struct audit { struct weston_compositor *c; struct wl_event_source *timer; struct wl_listener destroy; int phase; };
static int tick(void *data) {
 struct audit *a=data;
 const struct weston_windowed_output_api *api=weston_windowed_output_get_api(a->c,WESTON_WINDOWED_OUTPUT_HEADLESS);
 if(!api)return 0;
 if(a->phase++==0){
  if(api->create_head(a->c->primary_backend,"AUDIT-FAST")<0)return 0;
  wl_event_source_timer_update(a->timer,100);return 0;
 }
 struct weston_head *head=NULL;
 while((head=weston_compositor_iterate_heads(a->c,head))){
  if(strcmp(head->name,"AUDIT-FAST")||!head->output||!head->output->enabled)continue;
  struct weston_output *output=head->output;
  output->current_mode->refresh=144000;
  output->native_mode_copy.refresh=144000;
  struct wl_resource *resource;
  wl_resource_for_each(resource,&head->resource_list){
   wl_output_send_mode(resource,output->current_mode->flags,output->current_mode->width,output->current_mode->height,144000);
   if(wl_resource_get_version(resource)>=2)wl_output_send_done(resource);
  }
  fprintf(stderr,"AUDIT_OUTPUT name=%s mode=%dx%d refresh_mHz=%u\n",head->name,output->current_mode->width,output->current_mode->height,output->current_mode->refresh);
  return 0;
 }
 wl_event_source_timer_update(a->timer,100);return 0;
}
static void destroyed(struct wl_listener *listener, void *data){
 (void)data;struct audit *a=wl_container_of(listener,a,destroy);wl_event_source_remove(a->timer);wl_list_remove(&a->destroy.link);free(a);
}
WL_EXPORT int wet_module_init(struct weston_compositor *c,int *argc,char *argv[]){
 (void)argc;(void)argv;
 const char *private=getenv("HYPRV_TEST_PRIVATE_WAYLAND");if(!private||strcmp(private,"1"))return -1;
 struct audit *a=calloc(1,sizeof *a);if(!a)return -1;a->c=c;
 a->timer=wl_event_loop_add_timer(wl_display_get_event_loop(c->wl_display),tick,a);
 a->destroy.notify=destroyed;wl_signal_add(&c->destroy_signal,&a->destroy);
 wl_event_source_timer_update(a->timer,100);return 0;
}
