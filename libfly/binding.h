#ifndef JS_H_
#define JS_H_

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <v8.h>

extern "C"
{
  typedef struct
  {
    v8::Isolate *isolate;
    v8::Persistent<v8::Context> context;
    void *data;
    v8::Persistent<v8::Function> recv;
    const v8::FunctionCallbackInfo<v8::Value> *current_args;
  } js_runtime;

  typedef struct
  {
    const char *ptr;
    int len;
  } fly_buf;

  typedef struct
  {
    uint8_t *alloc_ptr; // Start of memory allocation (returned from `malloc()`).
    size_t alloc_len;   // Length of the memory allocation.
    uint8_t *data_ptr;  // Start of logical contents (within the allocation).
    size_t data_len;    // Length of logical contents.
  } fly_bytes;

  typedef struct
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
  } HeapStats;

} // extern "C"

struct Value;

enum class ValueTag : uint8_t
{
  None,
  Int32,
  String,
  Object,
  Array,
  ArrayBuffer,
  Uint8Array,
};

struct KeyValue
{
  const char *key;
  const Value *val;
};

struct ObjectPayload
{
  int32_t len;
  const KeyValue *pairs;
};

struct ArrayPayload
{
  size_t len;
  const Value *values;
};

union ValuePayload {
  int32_t Int32;
  const char *String;
  ArrayPayload Array;
  ObjectPayload Object;
  fly_buf ArrayBuffer;
  fly_bytes Uint8Array;
};

struct Value
{
  ValueTag tag;
  ValuePayload payload;
};

extern "C"
{
  extern const char *js_version();
  extern void js_init(fly_buf natives_blob, fly_buf snapshot_blob);

  extern js_runtime *js_runtime_new(void *);
  extern fly_buf js_snapshot_create(const char *);
  extern int js_send(const js_runtime *, fly_bytes);
  extern void js_set_response(const js_runtime *, const int32_t cmd_id, int32_t argc, const Value *argv);
  extern void js_set_return_value(const js_runtime *, const Value *);
  extern void *js_get_data(const js_runtime *);
  extern HeapStats js_runtime_heap_statistics(const js_runtime *);
  extern void js_eval(js_runtime *rt, const char *filename, const char *code);

  extern Value testy(const js_runtime *rt, const int32_t cmd_id, const char *name, int32_t argc, const Value *argv);

  extern void js_runtime_terminate(js_runtime *rt);

} // extern "C"

#endif // JS_H_