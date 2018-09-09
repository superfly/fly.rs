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

// Extracts a C string from a v8::V8 Utf8Value.
const char *ToCString(const v8::String::Utf8Value &value)
{
  return *value ? *value : "<string conversion failed>";
}

fly_buf str_to_buf(const v8::String::Utf8Value &src)
{
  char *data = static_cast<char *>(malloc(src.length()));
  memcpy(data, *src, src.length());
  return (fly_buf){data, src.length()};
}

fly_buf str_to_buf(v8::Isolate *iso, const v8::Local<v8::Value> &val)
{
  return str_to_buf(v8::String::Utf8Value(iso, val));
}

extern "C" void msg_from_js(const js_runtime *, fly_bytes);

// TODO: handle in rust
void Print(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  // CHECK_EQ(args.Length(), 1);
  auto *isolate = args.GetIsolate();
  v8::HandleScope handle_scope(isolate);
  v8::String::Utf8Value str(isolate, args[0]);
  const char *cstr = ToCString(str);
  printf("%s\n", cstr);
  fflush(stdout);
}

static v8::Local<v8::Uint8Array> ImportBuf(v8::Isolate *isolate, fly_bytes buf)
{
  if (buf.alloc_ptr == nullptr)
  {
    // If alloc_ptr isn't set, we memcpy.
    // This is currently used for flatbuffers created in Rust.
    auto ab = v8::ArrayBuffer::New(isolate, buf.data_len);
    memcpy(ab->GetContents().Data(), buf.data_ptr, buf.data_len);
    auto view = v8::Uint8Array::New(ab, 0, buf.data_len);
    return view;
  }
  else
  {
    auto ab = v8::ArrayBuffer::New(
        isolate, reinterpret_cast<void *>(buf.alloc_ptr), buf.alloc_len,
        v8::ArrayBufferCreationMode::kInternalized);
    auto view =
        v8::Uint8Array::New(ab, buf.data_ptr - buf.alloc_ptr, buf.data_len);
    return view;
  }
}

static fly_bytes ExportBuf(v8::Isolate *isolate,
                           v8::Local<v8::ArrayBufferView> view)
{
  auto ab = view->Buffer();
  auto contents = ab->Externalize();

  fly_bytes buf;
  buf.alloc_ptr = reinterpret_cast<uint8_t *>(contents.Data());
  buf.alloc_len = contents.ByteLength();
  buf.data_ptr = buf.alloc_ptr + view->ByteOffset();
  buf.data_len = view->ByteLength();

  // Prevent JS from modifying buffer contents after exporting.
  ab->Neuter();

  return buf;
}

static void FreeBuf(fly_bytes buf) { free(buf.alloc_ptr); }

void Send(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = static_cast<js_runtime *>(isolate->GetData(0));
  // DCHECK_EQ(d->isolate, isolate);

  v8::Locker locker(rt->isolate);
  v8::EscapableHandleScope handle_scope(isolate);

  // TODO: bring back checks
  // CHECK_EQ(args.Length(), 1);
  v8::Local<v8::Value> ab_v = args[0];
  // CHECK(ab_v->IsArrayBufferView());

  auto buf = ExportBuf(isolate, v8::Local<v8::ArrayBufferView>::Cast(ab_v));

  // DCHECK_EQ(d->currentArgs, nullptr);
  rt->current_args = &args;

  msg_from_js(rt, buf);

  // Buffer is only valid until the end of the callback.
  // TODO(piscisaureus):
  //   It's possible that data in the buffer is needed after the callback
  //   returns, e.g. when the handler offloads work to a thread pool, therefore
  //   make the callback responsible for releasing the buffer.
  FreeBuf(buf);

  rt->current_args = nullptr;
}

// Sets the recv callback.
void Recv(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = reinterpret_cast<js_runtime *>(isolate->GetData(0));
  // DCHECK_EQ(d->isolate, isolate);

  v8::HandleScope handle_scope(isolate);

  if (!rt->recv.IsEmpty())
  {
    isolate->ThrowException(v8_str(isolate, "libdeno.recv already called."));
    return;
  }

  v8::Local<v8::Value> v = args[0];
  // CHECK(v->IsFunction());
  v8::Local<v8::Function> func = v8::Local<v8::Function>::Cast(v);

  rt->recv.Reset(isolate, func);
}

extern "C"
{
  const char *js_version()
  {
    return v8::V8::GetVersion();
  }

  void js_init(fly_buf natives_blob, fly_buf snapshot_blob)
  {
    v8::StartupData natives;
    natives.data = natives_blob.ptr;
    natives.raw_size = natives_blob.len;
    v8::V8::SetNativesDataBlob(&natives);

    // TODO: make a custom snapshot
    v8::StartupData snapshot;
    snapshot.data = snapshot_blob.ptr;
    snapshot.raw_size = snapshot_blob.len;
    v8::V8::SetSnapshotDataBlob(&snapshot);

    // v8::V8::InitializeExternalStartupData(natives_blob, snapshot_blob);
    auto p = v8::platform::CreateDefaultPlatform();
    v8::V8::InitializePlatform(p);
    v8::V8::Initialize();
    return;
  }

  js_runtime *js_runtime_new(void *data)
  {
    js_runtime *rt = new js_runtime;

    v8::Isolate::CreateParams create_params;
    // TODO: create custom, better, allocator
    create_params.array_buffer_allocator = allocator;

    v8::Isolate *isolate = v8::Isolate::New(create_params);
    isolate->SetData(0, rt);
    rt->isolate = isolate;

    v8::Locker locker(isolate);
    v8::Isolate::Scope isolate_scope(isolate);
    {
      v8::HandleScope handle_scope(isolate);
      auto context = v8::Context::New(isolate, nullptr, v8::MaybeLocal<v8::ObjectTemplate>());

      v8::Context::Scope context_scope(context);

      auto global = context->Global();

      auto libfly = v8::Object::New(isolate);
      global->Set(context, v8_str(isolate, "libfly"), libfly).FromJust();

      auto print_tmpl = v8::FunctionTemplate::New(isolate, Print);
      auto print_val = print_tmpl->GetFunction(context).ToLocalChecked();
      libfly->Set(context, v8_str(isolate, "log"), print_val).FromJust();

      auto send_tmpl = v8::FunctionTemplate::New(isolate, Send);
      auto send_val = send_tmpl->GetFunction(context).ToLocalChecked();
      libfly->Set(context, v8_str(isolate, "send"), send_val).FromJust();

      auto recv_tmpl = v8::FunctionTemplate::New(isolate, Recv);
      auto recv_val = recv_tmpl->GetFunction(context).ToLocalChecked();
      libfly->Set(context, v8_str(isolate, "recv"), recv_val).FromJust();

      rt->context.Reset(rt->isolate, context);
    }

    rt->data = data;

    return rt;
  }

  void *js_get_data(const js_runtime *rt)
  {
    return rt->data;
  }

  int js_send(const js_runtime *rt, fly_bytes buf)
  {
    v8::Locker locker(rt->isolate);
    v8::Isolate::Scope isolate_scope(rt->isolate);
    v8::HandleScope handle_scope(rt->isolate);

    auto context = rt->context.Get(rt->isolate);
    v8::Context::Scope context_scope(context);

    v8::TryCatch try_catch(rt->isolate);

    auto recv = rt->recv.Get(rt->isolate);
    if (recv.IsEmpty())
    {
      // rt->last_exception = "libdeno.recv has not been called.";
      return 0;
    }

    v8::Local<v8::Value> args[1];
    args[0] = ImportBuf(rt->isolate, buf);
    recv->Call(context->Global(), 1, args);

    if (try_catch.HasCaught())
    {
      //   HandleException(context, try_catch.Exception());
      return 0;
    }

    return 1;
  }

  void js_set_response(const js_runtime *rt, fly_bytes buf)
  {
    auto ab = ImportBuf(rt->isolate, buf);
    rt->current_args->GetReturnValue().Set(ab);
  }

  void js_runtime_terminate(js_runtime *rt)
  {
    rt->context.Reset();
    rt->isolate->Dispose();
    free(rt);
  }

  void js_eval(js_runtime *rt, const char *filename, const char *code)
  {
    VALUE_SCOPE(rt->isolate, rt->context);
    // v8::TryCatch try_catch(rt->isolate);
    // try_catch.SetVerbose(true);

    v8::ScriptOrigin origin = v8::ScriptOrigin(v8_str(rt->isolate, filename));
    v8::MaybeLocal<v8::Script> script = v8::Script::Compile(
        ctx,
        v8_str(rt->isolate, code),
        &origin);

    if (script.IsEmpty())
    {
      printf("errrrr compiling!\n");
      return;
    }

    v8::MaybeLocal<v8::Value> result = script.ToLocalChecked()->Run(ctx);
    if (result.IsEmpty())
    {
      printf("errrrr evaluating!\n");
      return;
    }
  }

  // StartupData js_snapshot_create(const char *js)
  // {
  //   v8::StartupData data = v8::V8::CreateSnapshotDataBlob(js);
  //   return StartupData{data.data, data.raw_size};
  // }

  HeapStats js_runtime_heap_statistics(const js_runtime *rt)
  {
    v8::Isolate *isolate = rt->isolate;
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
        hs.does_zap_garbage() == 1,
    };
  }
}