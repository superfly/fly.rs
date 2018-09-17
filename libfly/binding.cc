#include <v8.h>
#include "bindings2.h"
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
  return v8::String::NewFromUtf8(iso, s);
}

// Extracts a C string from a v8::V8 Utf8Value.
// const char *ToCString(const v8::String::Utf8Value &value)
// {
//   return *value ? *value : "<string conversion failed>";
// }

char *str_to_char(const v8::String::Utf8Value &src)
{
  return strdup(*src);
}

fly_buf str_to_buf(const v8::String::Utf8Value &src)
{
  char *data = static_cast<char *>(malloc(src.length()));
  memcpy(data, *src, src.length());
  return (fly_buf){data, (size_t)src.length()};
}

fly_buf str_to_buf(v8::Isolate *iso, const v8::Local<v8::Value> &val)
{
  return str_to_buf(v8::String::Utf8Value(iso, val));
}

extern "C" void msg_from_js(const js_runtime *, Message);

// TODO: handle in rust
void Print(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  printf("got print\n");
  // CHECK_EQ(args.Length(), 1);
  auto *isolate = args.GetIsolate();
  v8::HandleScope handle_scope(isolate);
  v8::String::Utf8Value str(isolate, args[0]);
  // const char *cstr = ToCString(str);
  printf("%s\n", *str);
  fflush(stdout);
}

Value v8_to_value(const js_runtime *rt, v8::Local<v8::Context> ctx, v8::Local<v8::Value> v)
{
  // Value val;
  if (v->IsInt32())
  {
    return Value::Int32(v->Int32Value(ctx).FromJust());
  }
  else if (v->IsString())
  {
    return Value::String(strdup(*v8::String::Utf8Value(rt->isolate, v)));
  }
  else if (v->IsUint8Array())
  {
    auto view = v8::Local<v8::ArrayBufferView>::Cast(v);
    auto ab = view->Buffer();
    auto contents = ab->Externalize();
    fly_bytes buf;
    buf.alloc_ptr = reinterpret_cast<uint8_t *>(contents.Data());
    buf.alloc_len = contents.ByteLength();
    buf.data_ptr = buf.alloc_ptr + view->ByteOffset();
    buf.data_len = view->ByteLength();
    ab->Neuter();
    return Value::Uint8Array(buf);
  }
  else if (v->IsObject())
  {
    auto obj = v8::Local<v8::Object>::Cast(v);
    auto props = obj->GetOwnPropertyNames(ctx).ToLocalChecked();
    auto len = props->Length();
    KeyValue *kvs = new KeyValue[len]; // leak
    for (int i = 0; i < len; ++i)
    {
      auto key = props->Get(i);
      Value *val = new Value; // leak
      *val = v8_to_value(rt, ctx, obj->Get(key));
      kvs[i] = KeyValue{
          str_to_char(v8::String::Utf8Value(rt->isolate, key)),
          val};
    }
    return Value::Object(len, kvs);
  }
  return Value::None();
}

v8::Local<v8::Value> arg_to_value(const js_runtime *rt, Value val)
{
  switch (val.tag)
  {
  case Value::Tag::None:
    return v8::Undefined(rt->isolate);
  case Value::Tag::Int32:
    return v8::Integer::New(rt->isolate, val.int32._0);
  case Value::Tag::String:
    // printf("GOT A STRING VALUE: %s\n", t.payload.String);
    return v8_str(rt->isolate, strdup(val.string._0));
  case Value::Tag::Array:
  {
    auto arr = v8::Array::New(rt->isolate, val.array.len);
    for (size_t idx = 0; idx < val.array.len; idx++)
    {
      auto item = val.array.values[idx];
      arr->Set(v8::Int32::New(rt->isolate, idx), arg_to_value(rt, item));
    }
    return arr;
  }

  case Value::Tag::ArrayBuffer:
  {
    v8::Local<v8::ArrayBuffer> ab = v8::ArrayBuffer::New(rt->isolate, val.array_buffer._0.len);
    memcpy(ab->GetContents().Data(), val.array_buffer._0.ptr, val.array_buffer._0.len);
    return ab;
  }
  case Value::Tag::Uint8Array:
  {
    v8::Local<v8::ArrayBuffer> ab = v8::ArrayBuffer::New(rt->isolate, val.uint8_array._0.data_len);
    memcpy(ab->GetContents().Data(), val.uint8_array._0.data_ptr, val.uint8_array._0.data_len);
    return v8::Uint8Array::New(ab, 0, val.uint8_array._0.data_len);
  }
  case Value::Tag::Object:
  {
    auto obj = v8::Object::New(rt->isolate);
    for (int idx = 0; idx < val.object.len; idx++)
    {
      auto pair = val.object.pairs[idx];
      // printf("key idx %i => %s\n", idx, pair.key);

      obj->Set(v8_str(rt->isolate, strdup(pair.key)), arg_to_value(rt, *pair.val));
    }
    return obj;
  }
  }
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

void Send(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = static_cast<js_runtime *>(isolate->GetData(0));
  // DCHECK_EQ(d->isolate, isolate);

  v8::Locker locker(rt->isolate);
  v8::EscapableHandleScope handle_scope(isolate);

  auto context = rt->context.Get(rt->isolate);
  v8::Context::Scope context_scope(context);

  // TODO: bring back checks
  // CHECK_EQ(args.Length(), 1);
  // v8::Local<v8::Value> ab_v = args[0];
  // CHECK(ab_v->IsArrayBufferView());

  // auto buf = ExportBuf(isolate, v8::Local<v8::ArrayBufferView>::Cast(ab_v));

  // DCHECK_EQ(d->currentArgs, nullptr);
  rt->current_args = &args;

  auto obj = v8::Local<v8::Object>::Cast(args[0]);

  uint32_t id = obj->Get(v8_str(rt->isolate, "id"))->IntegerValue(context).FromJust();
  auto kind = static_cast<MessageKind>(obj->Get(v8_str(rt->isolate, "type"))->IntegerValue(context).FromJust());
  auto sync = obj->Get(v8_str(rt->isolate, "id"))->BooleanValue(context).FromJust();

  auto payload = v8_to_value(rt, context, obj->Get(v8_str(rt->isolate, "payload")));

  // int argc = args.Length();
  // int rust_argc = argc > 0 ? argc - 3 : 0;
  // Value *argv = new Value[rust_argc];

  // for (int i = 0; i < rust_argc; i++)
  // {
  //   auto arg_idx = i + 3;
  //   auto arg = args[arg_idx];
  //   argv[i] = v8_to_value(rt, context, arg);
  // }

  // auto cmd_id = args[0]->Int32Value(context).FromJust();
  // auto type = str_to_char(v8::String::Utf8Value(rt->isolate, args[1]));
  // auto sync = args[2]->BooleanValue(context).FromJust();

  // msg_from_js(rt, cmd_id, name, sync, rust_argc, argv);
  msg_from_js(rt, Message{id, sync, kind, payload});

  // Buffer is only valid until the end of the callback.
  // TODO(piscisaureus):
  //   It's possible that data in the buffer is needed after the callback
  //   returns, e.g. when the handler offloads work to a thread pool, therefore
  //   make the callback responsible for releasing the buffer.
  // FreeBuf(buf);

  rt->current_args = nullptr;
  // delete[] argv;
}

// void Send(const v8::FunctionCallbackInfo<v8::Value> &args)
// {
//   v8::Isolate *isolate = args.GetIsolate();
//   js_runtime *rt = static_cast<js_runtime *>(isolate->GetData(0));
//   // DCHECK_EQ(d->isolate, isolate);

//   v8::Locker locker(rt->isolate);
//   v8::EscapableHandleScope handle_scope(isolate);

//   // TODO: bring back checks
//   // CHECK_EQ(args.Length(), 1);
//   v8::Local<v8::Value> ab_v = args[0];
//   // CHECK(ab_v->IsArrayBufferView());

//   auto buf = ExportBuf(isolate, v8::Local<v8::ArrayBufferView>::Cast(ab_v));

//   // DCHECK_EQ(d->currentArgs, nullptr);
//   rt->current_args = &args;

//   msg_from_js(rt, buf);

//   // Buffer is only valid until the end of the callback.
//   // TODO(piscisaureus):
//   //   It's possible that data in the buffer is needed after the callback
//   //   returns, e.g. when the handler offloads work to a thread pool, therefore
//   //   make the callback responsible for releasing the buffer.
//   FreeBuf(buf);

//   rt->current_args = nullptr;
// }

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

intptr_t ext_refs[] = {
    reinterpret_cast<intptr_t>(Print),
    reinterpret_cast<intptr_t>(Send),
    reinterpret_cast<intptr_t>(Recv),
    0};

void InitContext(v8::Isolate *isolate, v8::Local<v8::Context> context, const char *filename, const char *code)
{
  v8::HandleScope handle_scope(isolate);
  v8::Context::Scope context_scope(context);

  auto fly = v8::Object::New(isolate);
  context->Global()->Set(context, v8_str(isolate, "libfly"), fly).FromJust();

  auto print_tmpl = v8::FunctionTemplate::New(isolate, Print);
  auto print_val = print_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "log"), print_val).FromJust();

  auto send_tmpl = v8::FunctionTemplate::New(isolate, Send);
  auto send_val = send_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "send"), send_val).FromJust();

  auto recv_tmpl = v8::FunctionTemplate::New(isolate, Recv);
  auto recv_val = recv_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "recv"), recv_val).FromJust();
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
    natives.data = (char *)natives_blob.ptr;
    natives.raw_size = natives_blob.len;
    v8::V8::SetNativesDataBlob(&natives);

    // TODO: make a custom snapshot
    v8::StartupData snapshot;
    snapshot.data = (char *)snapshot_blob.ptr;
    snapshot.raw_size = snapshot_blob.len;
    v8::V8::SetSnapshotDataBlob(&snapshot);

    // v8::V8::InitializeExternalStartupData(natives_blob, snapshot_blob);
    auto p = v8::platform::CreateDefaultPlatform();
    v8::V8::InitializePlatform(p);
    v8::V8::Initialize();
    return;
  }

  const js_runtime *js_runtime_new(fly_buf snapshot, void *data)
  {
    js_runtime *rt = new js_runtime;

    v8::Isolate::CreateParams create_params;

    if (snapshot.len > 0)
    {
      v8::StartupData blob;
      blob.data = snapshot.ptr;
      blob.raw_size = static_cast<int>(snapshot.len);

      create_params.snapshot_blob = &blob;
      create_params.external_references = ext_refs;
    }

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

      InitContext(isolate, context, nullptr, nullptr);

      rt->context.Reset(rt->isolate, context);
    }

    rt->data = data;

    return rt;
  }

  const void *js_get_data(const js_runtime *rt)
  {
    return rt->data;
  }

  Value js_send(const js_runtime *rt, MessageKind kind, Value cmd)
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
      return Value::None();
    }

    v8::Local<v8::Value> args[2];
    args[0] = v8::Int32::New(rt->isolate, static_cast<int32_t>(kind));
    args[1] = arg_to_value(rt, cmd);

    v8::MaybeLocal<v8::Value> res = recv->Call(v8::Undefined(rt->isolate), 2, args);
    if (res.IsEmpty())
    {
      printf("Empty res :/\n");
      return Value::None();
    }

    if (try_catch.HasCaught())
    {
      //   HandleException(context, try_catch.Exception());
      printf("CAUGHT AN EXCEPTION :/\n");
      return Value::None();
    }
    // delete[] args;
    return v8_to_value(rt, context, res.ToLocalChecked());
  }

  // void js_set_response(const js_runtime *, const int32_t cmd_id, int32_t argc, const Value *argv)
  // {
  //   rt->
  //   auto ab = ImportBuf(rt->isolate, buf);
  //   rt->current_args->GetReturnValue().Set(ab);
  // }

  void js_set_return_value(const js_runtime *rt, Value val)
  {
    rt->current_args->GetReturnValue().Set(arg_to_value(rt, val));
  }

  void js_runtime_terminate(js_runtime *rt)
  {
    rt->context.Reset();
    rt->isolate->Dispose();
    free(rt);
  }

  void js_eval(const js_runtime *rt, const char *filename, const char *code)
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

  js_heap_stats js_runtime_heap_statistics(const js_runtime *rt)
  {
    v8::Isolate *isolate = rt->isolate;
    v8::HeapStatistics hs;
    isolate->GetHeapStatistics(&hs);
    return js_heap_stats{
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

  // Value testy(const js_runtime *rt, const int32_t cmd_id, const char *name, int32_t argc, const Value *argv)
  Value testy(const js_runtime *rt, Message msg)
  {
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
      return Value::None();
    }

    auto msgobj = v8::Object::New(rt->isolate);
    msgobj->Set(v8_str(rt->isolate, "id"), v8::Int32::New(rt->isolate, msg.id));
    msgobj->Set(v8_str(rt->isolate, "sync"), v8::Boolean::New(rt->isolate, msg.sync));
    msgobj->Set(v8_str(rt->isolate, "type"), v8::Int32::New(rt->isolate, static_cast<int32_t>(msg.kind)));
    msgobj->Set(v8_str(rt->isolate, "payload"), arg_to_value(rt, msg.value));

    int length = 1;
    v8::Local<v8::Value> args[1];

    args[0] = msgobj;

    v8::MaybeLocal<v8::Value> res = recv->Call(v8::Undefined(rt->isolate), length, args);
    if (res.IsEmpty())
    {
      printf("Empty res :/\n");
      return Value::None();
    }

    if (try_catch.HasCaught())
    {
      //   HandleException(context, try_catch.Exception());
      printf("CAUGHT AN EXCEPTION :/\n");
      return Value::None();
    }

    // delete[] args;
    return v8_to_value(rt, context, res.ToLocalChecked());
  }

  fly_buf js_create_snapshot(const char *filename, const char *code)
  {
    v8::StartupData blob;
    {
      v8::SnapshotCreator creator(ext_refs);
      v8::Isolate *isolate = creator.GetIsolate();
      // js_runtime *rt = new js_runtime;
      // isolate->SetData(0, rt);
      // rt->isolate = isolate;
      {
        v8::HandleScope handle_scope(isolate);
        v8::Local<v8::Context> context = v8::Context::New(isolate);

        v8::Context::Scope context_scope(context);

        auto fly = v8::Object::New(isolate);
        context->Global()->Set(context, v8_str(isolate, "libfly"), fly).FromJust();

        auto print_tmpl = v8::FunctionTemplate::New(isolate, Print);
        auto print_val = print_tmpl->GetFunction(context).ToLocalChecked();
        fly->Set(context, v8_str(isolate, "log"), print_val);

        auto send_tmpl = v8::FunctionTemplate::New(isolate, Send);
        auto send_val = send_tmpl->GetFunction(context).ToLocalChecked();
        fly->Set(context, v8_str(isolate, "send"), send_val);

        auto recv_tmpl = v8::FunctionTemplate::New(isolate, Recv);
        auto recv_val = recv_tmpl->GetFunction(context).ToLocalChecked();
        fly->Set(context, v8_str(isolate, "recv"), recv_val);

        // rt->context.Reset(isolate, context);

        v8::ScriptOrigin origin = v8::ScriptOrigin(v8_str(isolate, filename));
        v8::MaybeLocal<v8::Script> script = v8::Script::Compile(
            context,
            v8_str(isolate, code),
            &origin);

        if (script.IsEmpty())
        {
          printf("errrrr compiling!\n");
          exit(1);
        }

        v8::MaybeLocal<v8::Value> result = script.ToLocalChecked()->Run(context);
        if (result.IsEmpty())
        {
          printf("errrrr evaluating!\n");
          exit(1);
        }

        creator.SetDefaultContext(context);
        // rt->context.Reset();
        // rt->recv.Reset();
      }
      blob =
          creator.CreateBlob(v8::SnapshotCreator::FunctionCodeHandling::kClear);
    }

    return fly_buf{blob.data, static_cast<size_t>(blob.raw_size)};
  }
}
