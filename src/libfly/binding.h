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

struct Value;

enum class ValueTag : uint8_t
{
  Int32,
  String,
  KeyValues,
};

struct KeyValue
{
  const char *key;
  const Value *val;
};

struct KeyValuePayload
{
  int32_t len;
  const KeyValue *pairs;
};

union ValuePayload {
  int32_t Int32;
  const char *String;
  KeyValuePayload KeyValues;
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

  extern Value testy(const js_runtime *rt, const int32_t cmd_id, const char *name, int32_t argc, const Value *argv)
  {
    // printf("testy, cmd_id: %i, name: %s\n", cmd_id, name);
    // Value *argv = new Value[argc]; //reinterpret_cast<Value *>(malloc(sizeof(Value) * argc));
    // memcpy(argv, rust_argv, argc * sizeof *argv);
    v8::Locker locker(rt->isolate);
    v8::Isolate::Scope isolate_scope(rt->isolate);
    v8::HandleScope handle_scope(rt->isolate);

    v8::Local<v8::Context> context = rt->context.Get(rt->isolate);
    v8::Context::Scope context_scope(context);

    v8::TryCatch try_catch(rt->isolate);
    try_catch.SetVerbose(true);

    v8::Local<v8::Function> recv = rt->recv.Get(rt->isolate);
    if (recv.IsEmpty())
    {
      // rt->last_exception = "libdeno.recv has not been called.";
      return Value{ValueTag::Int32, ValuePayload{}};
    }

    int length = argc + 2;
    v8::Local<v8::Value> args[length];

    args[0] = v8::Integer::New(rt->isolate, cmd_id);
    args[1] = v8_str(rt->isolate, strdup(name));

    for (int i = 0; i < argc; i++)
    {
      auto args_idx = i + 2;
      Value t = argv[i];
      // printf("value idx: %i tag: %i\n", i, t.tag);

      switch (t.tag)
      {
      case ValueTag::Int32:
        args[args_idx] = v8::Integer::New(rt->isolate, t.payload.Int32);
        break;
      case ValueTag::String:
        // printf("GOT A STRING VALUE: %s\n", t.payload.String);
        args[args_idx] = v8_str(rt->isolate, strdup(t.payload.String));
        break;
      case ValueTag::KeyValues:
        // printf("GOT AN OBJECT len: %i\n", t.payload.KeyValues.len);
        auto obj = v8::Object::New(rt->isolate);
        for (int idx = 0; idx < t.payload.KeyValues.len; idx++)
        {
          auto pair = t.payload.KeyValues.pairs[idx];
          // printf("key idx %i => %s\n", idx, pair.key);
          v8::Local<v8::Value> v8val;
          switch (pair.val->tag)
          {
          case ValueTag::Int32:
            // printf("value is a int32: %i\n", pair.val->payload.Int32);
            v8val = v8::Integer::New(rt->isolate, pair.val->payload.Int32);
            break;
          case ValueTag::String:
            // printf("value is a string: %s\n", pair.val->payload.String);
            v8val = v8_str(rt->isolate, strdup(pair.val->payload.String));
            break;
          }
          obj->Set(v8_str(rt->isolate, strdup(pair.key)), v8val);
        }
        args[args_idx] = obj;
        // printf("done with obj\n");
        break;
      }
    }
    v8::MaybeLocal<v8::Value> res = recv->Call(v8::Undefined(rt->isolate), length, args);
    if (res.IsEmpty())
    {
      printf("Empty res :/\n");
      return Value{ValueTag::Int32, ValuePayload{}};
    }

    if (try_catch.HasCaught())
    {
      //   HandleException(context, try_catch.Exception());
      printf("CAUGHT AN EXCEPTION :/\n");
      return Value{ValueTag::Int32, ValuePayload{}};
    }
    // delete[] args;
    return Value{ValueTag::Int32, ValuePayload{res.ToLocalChecked()->ToInt32(rt->isolate)->Value()}};
  }

  // extern Value js_current_arg_value(const js_runtime *rt, int i)
  // {
  //   const v8::FunctionCallbackInfo<v8::Value> args = *(rt->current_args);
  //   auto arg = args[i];

  //   v8::Locker locker(rt->isolate);
  //   v8::Isolate::Scope isolate_scope(rt->isolate);
  //   v8::HandleScope handle_scope(rt->isolate);

  //   auto context = rt->context.Get(rt->isolate);
  //   v8::Context::Scope context_scope(context);

  //   if (arg->IsInt32())
  //   {
  //     return Value{ValueTag::Integer, arg->Int32Value(context).FromJust()}; //v8::Int32::Cast(arg)->Value() }
  //   }
  //   // args
  //   // auto arg = args[i]->IsNumber();
  //   // if (arg.)
  //   // if
  //   //   arg
  // }

  extern void js_runtime_terminate(js_runtime *rt);

#ifdef __cplusplus
} // extern "C"
#endif
#endif // JS_H_