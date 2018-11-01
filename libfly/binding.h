#ifndef libfly
#define libfly

#include <cstdint>
#include <cstdlib>
#include <cstring>

struct fly_simple_buf
{
  const char *ptr;
  int len;
};

struct js_heap_stats
{
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

struct fly_buf
{
  uint8_t *alloc_ptr;
  uintptr_t alloc_len;
  uint8_t *data_ptr;
  uintptr_t data_len;
};

struct js_runtime;
typedef struct js_runtime runtime;

typedef void (*fly_recv_cb)(runtime *rt, fly_buf control_buf,
                            fly_buf data_buf);
typedef void (*fly_print_cb)(runtime *rt, int8_t lvl, const char *msg);

struct js_runtime_options
{
  fly_simple_buf snapshot;
  void *data;
  fly_recv_cb recv_cb;
  fly_print_cb print_cb;
  size_t soft_memory_limit;
  size_t hard_memory_limit;
};

extern "C"
{
  extern fly_simple_buf js_create_snapshot(const char *filename, const char *code);

  extern bool js_dump_heap_snapshot(const runtime *rt, const char *filename);

  extern bool js_eval(const runtime *rt, const char *filename, const char *code);

  extern const void *js_get_data(const runtime *rt);

  extern void js_init();

  extern js_heap_stats js_runtime_heap_statistics(const runtime *rt);

  extern const runtime *js_runtime_new(js_runtime_options);

  extern int js_send(const runtime *rt, fly_buf buf, fly_buf raw);

  extern void js_set_response(const runtime *rt, fly_buf buf);

  extern void js_runtime_dispose(const runtime *rt);

  extern const char *js_version();

} // extern "C"

#endif // libfly
