// Test-only LD_PRELOAD instrumentation. Never installed or used in the live session.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/syscall.h>
#include <sys/timerfd.h>
#include <time.h>
static int logfd=-1;
static __thread uint64_t begun;
static __thread void *draw_surface;
static uint64_t ns(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
__attribute__((constructor)) static void init(void){const char*p=getenv("SOAK_TRACE");if(p)logfd=open(p,O_WRONLY|O_CREAT|O_APPEND|O_CLOEXEC,0600);}
static void row(char type,void*surface,uint64_t elapsed,uint64_t detail){if(logfd<0)return;char b[160];int n=snprintf(b,sizeof b,"%c %llu %ld %p %llu %llu\n",type,(unsigned long long)ns(),syscall(SYS_gettid),surface,(unsigned long long)elapsed,(unsigned long long)detail);syscall(SYS_write,logfd,b,n);}
typedef void*(*lookup_fn)(void*,const char*);
static void*lookup(void*h,const char*n){static lookup_fn f;if(!f)f=(lookup_fn)dlvsym(RTLD_NEXT,"dlsym","GLIBC_2.2.5");return f(h,n);}
static int(*real_make)(void*,void*,void*,void*);
static int(*real_swap)(void*,void*);
static int(*real_damage_khr)(void*,void*,const int*,int);
static int(*real_damage_ext)(void*,void*,const int*,int);
static void*(*real_getproc)(const char*);
static int make(void*d,void*draw,void*read,void*ctx){int r=real_make(d,draw,read,ctx);if(r){begun=ns();draw_surface=draw;}return r;}
static void finish(void*s,uint64_t start,int ok){uint64_t now=ns();row('F',s,(begun&&draw_surface==s)?now-begun:0,now-start);if(!ok)row('E',s,0,0);begun=0;}
static int swap(void*d,void*s){uint64_t t=ns();int r=real_swap(d,s);finish(s,t,r);return r;}
static int damage_khr(void*d,void*s,const int*r,int n){uint64_t t=ns();int v=real_damage_khr(d,s,r,n);finish(s,t,v);return v;}
static int damage_ext(void*d,void*s,const int*r,int n){uint64_t t=ns();int v=real_damage_ext(d,s,r,n);finish(s,t,v);return v;}
static void*intercept(const char*n,void*p){if(!p)return p;
 if(!strcmp(n,"eglMakeCurrent")){real_make=p;return make;}
 if(!strcmp(n,"eglSwapBuffers")){real_swap=p;return swap;}
 if(!strcmp(n,"eglSwapBuffersWithDamageKHR")){real_damage_khr=p;return damage_khr;}
 if(!strcmp(n,"eglSwapBuffersWithDamageEXT")){real_damage_ext=p;return damage_ext;}
 return p;}
static void*getproc(const char*n){return intercept(n,real_getproc(n));}
void*dlsym(void*h,const char*n){void*p=lookup(h,n);if(p&&!strcmp(n,"eglGetProcAddress")){real_getproc=p;return getproc;}return intercept(n,p);}
int timerfd_create(int clock,int flags){static int(*f)(int,int);if(!f)f=lookup(RTLD_NEXT,"timerfd_create");int fd=f(clock,flags);row('C',(void*)(intptr_t)fd,clock,flags);return fd;}
int timerfd_settime(int fd,int flags,const struct itimerspec*n,struct itimerspec*old){static int(*f)(int,int,const struct itimerspec*,struct itimerspec*);if(!f)f=lookup(RTLD_NEXT,"timerfd_settime");int r=f(fd,flags,n,old);if(!r)row('T',(void*)(intptr_t)fd,(uint64_t)n->it_value.tv_sec*1000000000+n->it_value.tv_nsec,(uint64_t)n->it_interval.tv_sec*1000000000+n->it_interval.tv_nsec);return r;}
