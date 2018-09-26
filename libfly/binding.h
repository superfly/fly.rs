#ifndef libfly
#define libfly

/* Generated with cbindgen:0.6.3 */

// Auto-generated, don't edit!

#include <cstdint>
#include <cstdlib>
#include "runtime.h"

struct fly_simple_buf {
  const char *ptr;
  int len;
};

struct js_heap_stats {
  size_t total_heap_size;
  size_t total_heap_size_executable;
  size_t total_physical_size;
  size_t total_available_size;
  size_t used_heap_size;
  size_t heap_size_limit;
  size_t malloced_memory;
  size_t peak_malloced_memory;
  size_t number_of_native_contexts;
  size_t number_of_detached_contexts;
  bool does_zap_garbage;
  size_t externally_allocated;
};

struct fly_buf {
  uint8_t *alloc_ptr;
  uintptr_t alloc_len;
  uint8_t *data_ptr;
  uintptr_t data_len;
};

extern "C" {

extern fly_simple_buf js_create_snapshot(const char *filename, const char *code);

extern bool js_dump_heap_snapshot(const js_runtime *rt, const char *filename);

extern bool js_eval(const js_runtime *rt, const char *filename, const char *code);

extern const void *js_get_data(const js_runtime *rt);

extern void js_init(fly_simple_buf natives, fly_simple_buf snapshot);

extern js_heap_stats js_runtime_heap_statistics(const js_runtime *rt);

extern const js_runtime *js_runtime_new(fly_simple_buf snapshot, void *data);

extern int js_send(const js_runtime *rt, fly_buf buf, fly_buf raw);

extern void js_set_response(const js_runtime *rt, fly_buf buf);

extern const char *js_version();

} // extern "C"

#endif // libfly
