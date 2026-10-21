// LD_PRELOAD shim for sale-soon.mjs: the wall clock (CLOCK_REALTIME, gettimeofday, time) runs
// CLOCK_SHIFT_SECONDS ahead. Monotonic clocks are untouched, so timers and timeouts behave.
// Built on the fly: cc -shared -fPIC -o clock-shift.so clock-shift.c -ldl
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdlib.h>
#include <sys/time.h>
#include <time.h>

static long shift(void) { const char *s = getenv("CLOCK_SHIFT_SECONDS"); return s ? atol(s) : 0; }

int clock_gettime(clockid_t id, struct timespec *ts) {
  static int (*real)(clockid_t, struct timespec *);
  if (!real) real = dlsym(RTLD_NEXT, "clock_gettime");
  int r = real(id, ts);
  if (r == 0 && (id == CLOCK_REALTIME || id == CLOCK_REALTIME_COARSE)) ts->tv_sec += shift();
  return r;
}

int gettimeofday(struct timeval *tv, void *tz) {
  static int (*real)(struct timeval *, void *);
  if (!real) real = dlsym(RTLD_NEXT, "gettimeofday");
  int r = real(tv, tz);
  if (r == 0 && tv) tv->tv_sec += shift();
  return r;
}

time_t time(time_t *t) {
  struct timespec ts;
  clock_gettime(CLOCK_REALTIME, &ts);
  if (t) *t = ts.tv_sec;
  return ts.tv_sec;
}
