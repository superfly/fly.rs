#ifndef JS_H_
#define JS_H_

#include <stddef.h>
#include <stdint.h>
#include <v8.h>

#ifdef __cplusplus
extern "C"
{
#endif

  // typedef v8::Isolate *IsolatePtr;
  // typedef v8::Persistent<v8::Context> *PersistentContext;

  typedef struct
  {
    v8::Isolate *isolate;
    v8::Persistent<v8::Context> context;
  } js_runtime;

  // typedef v8::Persistent<v8::Value> js_value;

  typedef struct
  {
    js_runtime *runtime;
    v8::Persistent<v8::Value> value;
  } js_value;

  typedef struct
  {
    const char *ptr;
    int len;
  } fly_string;

  typedef fly_string StartupData;

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
    size_t does_zap_garbage;
  } HeapStats;

  extern const char *js_version();
  extern void js_init();
  // extern v8::Isolate *js_isolate_new(StartupData);
  extern js_runtime *js_runtime_new(StartupData startup_data);
  extern StartupData js_snapshot_create(const char *);
  extern HeapStats js_isolate_heap_statistics(v8::Isolate *);
  extern v8::Persistent<v8::Context> *js_context_new(v8::Isolate *isoptr);
  extern js_value *js_global(js_runtime *rt);
  extern void js_global_set_function(js_runtime *rt, const char *name, v8::FunctionCallback cb);
  extern void js_eval(js_runtime *rt, const char *code);
  extern js_runtime *js_callback_info_runtime(const v8::FunctionCallbackInfo<v8::Value> &info);
  extern js_value *js_callback_info_get(const v8::FunctionCallbackInfo<v8::Value> &info, int index);
  extern int js_callback_info_length(const v8::FunctionCallbackInfo<v8::Value> &info);
  extern fly_string js_value_to_string(js_value *);
  extern bool js_value_is_function(js_value *v);
  extern js_value *js_value_call(js_runtime *rt, js_value *v);
  extern int64_t js_value_to_i64(js_value *v);

  extern void js_runtime_release(js_runtime *rt);
  extern void js_value_release(js_value *v);

  extern bool js_value_set(js_value *v, const char *name, js_value *prop);
  extern js_value *js_function_new(js_runtime *rt, v8::FunctionCallback cb);

  extern bool js_value_is_object(js_value *v);
  extern int js_value_string_utf8_len(js_value *v);
  extern void js_value_string_write_utf8(js_value *v, char *buf, int len);

#ifdef __cplusplus
} // extern "C"
#endif
#endif // JS_H_