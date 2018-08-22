#include "binding.h"
#include <libplatform/libplatform.h>

#define ISOLATE_SCOPE(iso)                                                    \
  v8::Locker locker(iso);                /* Lock to current thread.        */ \
  v8::Isolate::Scope isolate_scope(iso); /* Assign isolate to this thread. */

#define VALUE_SCOPE(iso, ctxptr)                                          \
  ISOLATE_SCOPE(iso);                                                     \
  v8::HandleScope handle_scope(iso); /* Create a scope for handles.    */ \
  v8::Local<v8::Context> ctx(ctxptr.Get(iso));                            \
  v8::Context::Scope context_scope(ctx); /* Scope to this context.         */

auto allocator = v8::ArrayBuffer::Allocator::NewDefaultAllocator();

static inline v8::Local<v8::String> v8_str(v8::Isolate *iso, const char *s)
{
  return v8::String::NewFromUtf8(iso, s, v8::NewStringType::kNormal).ToLocalChecked();
}

fly_string DupString(const v8::String::Utf8Value &src)
{
  char *data = static_cast<char *>(malloc(src.length()));
  memcpy(data, *src, src.length());
  return (fly_string){data, src.length()};
}
fly_string DupString(v8::Isolate *iso, const v8::Local<v8::Value> &val)
{
  return DupString(v8::String::Utf8Value(iso, val));
}

extern "C"
{
  const char *js_version()
  {
    return v8::V8::GetVersion();
  }

  void js_init()
  {
    v8::V8::InitializeExternalStartupData("/Users/jerome/v8/v8/out/x64.debug/");
    auto p = v8::platform::CreateDefaultPlatform();
    v8::V8::InitializePlatform(p);
    v8::V8::Initialize();
    return;
  }

  js_runtime *js_runtime_new(StartupData startup_data)
  {
    js_runtime *rt = new js_runtime;

    v8::Isolate::CreateParams create_params;
    create_params.array_buffer_allocator = allocator;
    v8::StartupData *data = new v8::StartupData;
    data->data = startup_data.ptr;
    data->raw_size = startup_data.len;
    create_params.snapshot_blob = data;

    v8::Isolate *isolate = v8::Isolate::New(create_params);
    isolate->SetData(0, rt);
    rt->isolate = isolate;

    v8::Locker locker(isolate);
    v8::Isolate::Scope isolate_scope(isolate);
    {
      v8::HandleScope handle_scope(isolate);
      auto context = v8::Context::New(isolate, nullptr, v8::MaybeLocal<v8::ObjectTemplate>());
      rt->context.Reset(rt->isolate, context);
    }

    return rt;
  }

  void js_runtime_release(js_runtime *rt)
  {
    rt->context.Reset();
    rt->isolate->Dispose();
    free(rt);
  }

  js_runtime *js_callback_info_runtime(const v8::FunctionCallbackInfo<v8::Value> &info)
  {
    v8::Local<v8::External> ext = info.Data().As<v8::External>();
    return static_cast<js_runtime *>(ext->Value());
  }

  js_value *js_global(js_runtime *rt)
  {
    VALUE_SCOPE(rt->isolate, rt->context);

    auto global = ctx->Global();
    js_value *v = new js_value;
    v->value.Reset(rt->isolate, global);
    v->runtime = rt;
    return v;
  }

  bool js_value_set(js_value *v, const char *name, js_value *prop)
  {
    VALUE_SCOPE(v->runtime->isolate, v->runtime->context);

    auto maybeObj = v->value.Get(v->runtime->isolate);
    if (!maybeObj->IsObject())
      return false;

    auto obj = maybeObj->ToObject(v->runtime->isolate);
    return obj->Set(v8_str(v->runtime->isolate, name), prop->value.Get(prop->runtime->isolate));
  }

  js_value *js_function_new(js_runtime *rt, v8::FunctionCallback cb)
  {
    VALUE_SCOPE(rt->isolate, rt->context);
    v8::Local<v8::External> data = v8::External::New(rt->isolate, rt);
    auto fn = v8::FunctionTemplate::New(rt->isolate, cb, data);
    auto val = fn->GetFunction(ctx).ToLocalChecked();
    js_value *v = new js_value;
    v->value.Reset(rt->isolate, val);
    v->runtime = rt;
    return v;
  }

  int js_callback_info_length(const v8::FunctionCallbackInfo<v8::Value> &info)
  {
    return info.Length();
  }

  js_value *js_callback_info_get(const v8::FunctionCallbackInfo<v8::Value> &info, int index)
  {
    if (info.Length() <= index)
    {
      return nullptr;
    }
    js_runtime *rt = js_callback_info_runtime(info);
    ISOLATE_SCOPE(rt->isolate);
    v8::HandleScope handle_scope(rt->isolate);
    js_value *v = new js_value;
    v->value.Reset(rt->isolate, info[index]);
    v->runtime = rt;
    return v;
  }

  fly_string js_value_to_string(js_value *v)
  {
    ISOLATE_SCOPE(v->runtime->isolate);
    v8::HandleScope hs(v->runtime->isolate);
    return DupString(v->runtime->isolate, v->value.Get(v->runtime->isolate));
  }

  bool js_value_is_function(js_value *v)
  {
    ISOLATE_SCOPE(v->runtime->isolate);
    v8::HandleScope hs(v->runtime->isolate);
    return v->value.Get(v->runtime->isolate)->IsFunction();
  }

  bool js_value_is_object(js_value *v)
  {
    ISOLATE_SCOPE(v->runtime->isolate);
    v8::HandleScope hs(v->runtime->isolate);
    return v->value.Get(v->runtime->isolate)->IsObject();
  }

  int64_t js_value_to_i64(js_value *v)
  {
    ISOLATE_SCOPE(v->runtime->isolate);
    v8::HandleScope hs(v->runtime->isolate);
    return v->value.Get(v->runtime->isolate)->IntegerValue();
    // if (val.IsNothing())
    // {
    //   return 0;
    // }
    // return val.ToChecked();
  }

  int js_value_string_utf8_len(js_value *v)
  {
    VALUE_SCOPE(v->runtime->isolate, v->runtime->context);
    auto value = v->value.Get(v->runtime->isolate);
    auto str = value->ToString();
    return str->Utf8Length();
  }

  void js_value_string_write_utf8(js_value *v, char *buf, int len)
  {
    VALUE_SCOPE(v->runtime->isolate, v->runtime->context);
    auto value = v->value.Get(v->runtime->isolate);
    auto str = value->ToString();
    str->WriteUtf8(v->runtime->isolate, buf, len);
  }

  js_value *js_value_call(js_runtime *rt, js_value *v)
  {
    VALUE_SCOPE(rt->isolate, rt->context);
    // v8::HandleScope handle_scope(v->runtime->isolate);
    // v8::Context::Scope context_scope(v->runtime->isolate->GetCurrentContext());
    v8::TryCatch try_catch(v->runtime->isolate);
    try_catch.SetVerbose(true);
    v8::Local<v8::Function> func = v8::Local<v8::Function>::Cast(v->value.Get(v->runtime->isolate));
    int argc = 0;
    v8::Local<v8::Value> *argv = new v8::Local<v8::Value>[argc];

    v8::MaybeLocal<v8::Value> result = func->Call(v8::Undefined(v->runtime->isolate), argc, argv);

    delete[] argv;

    if (result.IsEmpty())
    {
      printf("FAILED IN C\n");
      return nullptr;
    }

    js_value *new_val = new js_value;
    new_val->runtime = v->runtime;
    new_val->value.Reset(v->runtime->isolate, result.ToLocalChecked());

    return new_val;
  }

  void js_value_release(js_value *v)
  {
    v->value.Reset();
    free(v);
  }

  void js_eval(js_runtime *rt, const char *code)
  {
    VALUE_SCOPE(rt->isolate, rt->context);
    // v8::TryCatch try_catch(rt->isolate);
    // try_catch.SetVerbose(true);

    v8::Local<v8::Script> script = v8::Script::Compile(
        v8_str(rt->isolate, code),
        v8_str(rt->isolate, "(no file)"));

    if (script.IsEmpty())
    {
      printf("errrrr compiling!\n");
      return;
    }

    v8::Local<v8::Value> result = script->Run();
    if (result.IsEmpty())
    {
      printf("errrrr evaluating!\n");
      return;
    }
  }

  StartupData js_snapshot_create(const char *js)
  {
    v8::StartupData data = v8::V8::CreateSnapshotDataBlob(js);
    return StartupData{data.data, data.raw_size};
  }

  HeapStats js_isolate_heap_statistics(v8::Isolate *iso)
  {
    v8::Isolate *isolate = static_cast<v8::Isolate *>(iso);
    v8::HeapStatistics hs;
    isolate->GetHeapStatistics(&hs);
    return HeapStats{
        hs.total_heap_size(),
        hs.total_heap_size_executable(),
        hs.total_physical_size(),
        hs.total_available_size(),
        hs.used_heap_size(),
        hs.heap_size_limit(),
        hs.malloced_memory(),
        hs.peak_malloced_memory(),
        hs.number_of_native_contexts(),
        hs.number_of_detached_contexts(),
        hs.does_zap_garbage()};
  }
}