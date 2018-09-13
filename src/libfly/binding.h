#ifndef JS_H_
#define JS_H_

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <v8.h>

static inline v8::Local<v8::String> v8_str(v8::Isolate *iso, const char *s)
{
  return v8::String::NewFromUtf8(iso, s);
}

struct buf
{
  const char *ptr;
  int len;
};

struct Value;

enum class ValueTag : uint8_t
{
  None,
  Int32,
  String,
  Object,
  ArrayBuffer,
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

union ValuePayload {
  int32_t Int32;
  const char *String;
  ObjectPayload Object;
  buf ArrayBuffer;
};

struct Value
{
  ValueTag tag;
  ValuePayload payload;
};

#ifdef __cplusplus
extern "C"
{
#endif

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

  extern const char *js_version();
  extern void js_init(fly_buf natives_blob, fly_buf snapshot_blob);

  extern js_runtime *js_runtime_new(void *);
  extern fly_buf js_snapshot_create(const char *);
  extern int js_send(const js_runtime *, fly_bytes);
  extern void js_set_response(const js_runtime *, fly_bytes);
  extern void js_set_return_value(const js_runtime *, const Value *);
  extern void *js_get_data(const js_runtime *);
  extern HeapStats js_runtime_heap_statistics(const js_runtime *);
  extern void js_eval(js_runtime *rt, const char *filename, const char *code);

  extern Value testy(const js_runtime *rt, const int32_t cmd_id, const char *name, int32_t argc, const Value *argv);

  extern void js_runtime_terminate(js_runtime *rt);

#ifdef __cplusplus
} // extern "C"
#endif
Value v8_to_value(const js_runtime *rt, v8::Local<v8::Context> ctx, v8::Local<v8::Value> v)
{
  ValueTag tag;
  ValuePayload payload;
  if (v->IsInt32())
  {
    tag = ValueTag::Int32;
    payload = ValuePayload{v->Int32Value(ctx).FromJust()};
  }
  else if (v->IsString())
  {
    tag = ValueTag::String;
    payload = ValuePayload{.String = strdup(*v8::String::Utf8Value(rt->isolate, v))};
  }
  else
  {
    return Value{ValueTag::None, ValuePayload{}};
  }
  return Value{tag, payload};
}

v8::Local<v8::Value> arg_to_value(const js_runtime *rt, ValueTag tag, ValuePayload payload)
{
  switch (tag)
  {
  case ValueTag::None:
    return v8::Undefined(rt->isolate);
  case ValueTag::Int32:
    return v8::Integer::New(rt->isolate, payload.Int32);
  case ValueTag::String:
    // printf("GOT A STRING VALUE: %s\n", t.payload.String);
    return v8_str(rt->isolate, strdup(payload.String));
  case ValueTag::ArrayBuffer:
  {
    v8::Local<v8::ArrayBuffer> ab = v8::ArrayBuffer::New(rt->isolate, payload.ArrayBuffer.len);
    memcpy(ab->GetContents().Data(), payload.ArrayBuffer.ptr, payload.ArrayBuffer.len);
    return ab;
  }
  case ValueTag::Object:
  {
    auto obj = v8::Object::New(rt->isolate);
    for (int idx = 0; idx < payload.Object.len; idx++)
    {
      auto pair = payload.Object.pairs[idx];
      // printf("key idx %i => %s\n", idx, pair.key);

      obj->Set(v8_str(rt->isolate, strdup(pair.key)), arg_to_value(rt, pair.val->tag, pair.val->payload));
    }
    return obj;
  }
  }
}
#endif // JS_H_