// Input is sent ONLY to the explicit private socket passed in argv[1].
#include <wayland-client.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "pointer-protocol.h"
static struct zwlr_virtual_pointer_manager_v1 *manager;
static void global(void*d,struct wl_registry*r,uint32_t name,const char*iface,uint32_t version){if(!strcmp(iface,zwlr_virtual_pointer_manager_v1_interface.name))manager=wl_registry_bind(r,name,&zwlr_virtual_pointer_manager_v1_interface,version<2?version:2);}
static void gone(void*d,struct wl_registry*r,uint32_t n){}
static const struct wl_registry_listener listener={global,gone};
int main(int argc,char**argv){if(argc!=2||!strstr(argv[1],"/hv-soak-"))return 2;struct wl_display*d=wl_display_connect(argv[1]);if(!d)return 3;struct wl_registry*r=wl_display_get_registry(d);wl_registry_add_listener(r,&listener,0);wl_display_roundtrip(d);if(!manager)return 4;struct zwlr_virtual_pointer_v1*p=zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager,0);wl_display_roundtrip(d);char line[128],op[32];int x,y,v;while(fgets(line,sizeof line,stdin)){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);unsigned ms=t.tv_sec*1000+t.tv_nsec/1000000;
 if(sscanf(line,"%31s %d %d",op,&x,&y)>=2){if(!strcmp(op,"move"))zwlr_virtual_pointer_v1_motion_absolute(p,ms,x,y,1280,720);else if(!strcmp(op,"button"))zwlr_virtual_pointer_v1_button(p,ms,0x110,x);else if(!strcmp(op,"scroll")){zwlr_virtual_pointer_v1_axis_source(p,WL_POINTER_AXIS_SOURCE_WHEEL);zwlr_virtual_pointer_v1_axis(p,ms,WL_POINTER_AXIS_VERTICAL_SCROLL,wl_fixed_from_int(x));}}
 zwlr_virtual_pointer_v1_frame(p);wl_display_roundtrip(d);puts("ok");fflush(stdout);}
 zwlr_virtual_pointer_v1_destroy(p);wl_display_disconnect(d);return 0;}
