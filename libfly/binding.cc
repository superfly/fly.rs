#include <v8.h>
#include "binding.h"
#include "runtime.h"
#include <libplatform/libplatform.h>
#include "allocator.h"
#include "file_output_stream.h"

#define ISOLATE_SCOPE(iso)                                                    \
  v8::Locker locker(iso);                /* Lock to current thread.        */ \
  v8::Isolate::Scope isolate_scope(iso); /* Assign isolate to this thread. */

#define VALUE_SCOPE(iso, ctxptr)                                          \
  ISOLATE_SCOPE(iso);                                                     \
  v8::HandleScope handle_scope(iso); /* Create a scope for handles.    */ \
  v8::Local<v8::Context> ctx(ctxptr.Get(iso));                            \
  v8::Context::Scope context_scope(ctx); /* Scope to this context.         */

struct InternalFieldData
{
  uint32_t data;
};

std::vector<InternalFieldData *> deserialized_data;

void DeserializeInternalFields(v8::Local<v8::Object> holder, int index,
                               v8::StartupData payload, void *data)
{
  // DCHECK_EQ(data, nullptr);
  if (payload.raw_size == 0)
  {
    holder->SetAlignedPointerInInternalField(index, nullptr);
    return;
  }
  InternalFieldData *embedder_field = new InternalFieldData{0};
  memcpy(embedder_field, payload.data, payload.raw_size);
  holder->SetAlignedPointerInInternalField(index, embedder_field);
  deserialized_data.push_back(embedder_field);
}

v8::StartupData SerializeInternalFields(v8::Local<v8::Object> holder, int index,
                                        void *data)
{
  // DCHECK_EQ(data, nullptr);
  InternalFieldData *embedder_field = static_cast<InternalFieldData *>(
      holder->GetAlignedPointerFromInternalField(index));
  if (embedder_field == nullptr)
    return {nullptr, 0};
  int size = sizeof(*embedder_field);
  char *payload = new char[size];
  // We simply use memcpy to serialize the content.
  memcpy(payload, embedder_field, size);
  return {payload, size};
}

extern "C" void free_fly_buf(fly_buf);

static inline v8::Local<v8::String> v8_str(v8::Isolate *iso, const char *s)
{
  return v8::String::NewFromUtf8(iso, s);
}
static inline v8::Local<v8::String> v8_str(const char *s)
{
  return v8::String::NewFromUtf8(v8::Isolate::GetCurrent(), s,
                                 v8::NewStringType::kNormal)
      .ToLocalChecked();
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

js_runtime *FromIsolate(v8::Isolate *isolate)
{
  return static_cast<js_runtime *>(isolate->GetData(0));
}

void HandleExceptionStr(v8::Local<v8::Context> context,
                        v8::Local<v8::Value> exception,
                        std::string *exception_str)
{
  auto *isolate = context->GetIsolate();
  js_runtime *rt = FromIsolate(isolate);

  v8::HandleScope handle_scope(isolate);
  v8::Context::Scope context_scope(context);

  auto message = v8::Exception::CreateMessage(isolate, exception);
  auto stack_trace = message->GetStackTrace();
  auto line =
      v8::Integer::New(isolate, message->GetLineNumber(context).FromJust());
  auto column =
      v8::Integer::New(isolate, message->GetStartColumn(context).FromJust());

  if (rt != nullptr)
  {
    auto global_error_handler = rt->global_error_handler.Get(isolate);

    if (!global_error_handler.IsEmpty())
    {
      // global_error_handler is set so we try to handle the exception in
      // javascript.
      v8::Local<v8::Value> args[5];
      args[0] = exception->ToString(context).ToLocalChecked();
      args[1] = message->GetScriptResourceName();
      args[2] = line;
      args[3] = column;
      args[4] = exception;
      global_error_handler->Call(context->Global(), 5, args);
      /* message, source, lineno, colno, error */

      return;
    }
  }

  char buf[12 * 1024];
  if (!stack_trace.IsEmpty())
  {
    // No javascript error handler, but we do have a stack trace. Format it
    // into a string and add to last_exception.
    std::string msg;
    v8::String::Utf8Value exceptionStr(isolate, exception);
    msg += *exceptionStr;
    msg += "\n";

    for (int i = 0; i < stack_trace->GetFrameCount(); ++i)
    {
      auto frame = stack_trace->GetFrame(isolate, i);
      v8::String::Utf8Value script_name(isolate, frame->GetScriptName());
      int l = frame->GetLineNumber();
      int c = frame->GetColumn();
      snprintf(buf, sizeof(buf), "%s %d:%d\n", *script_name, l, c);
      msg += buf;
    }
    *exception_str += msg;
  }
  else
  {
    // No javascript error handler, no stack trace. Format the little info we
    // have into a string and add to last_exception.
    v8::String::Utf8Value exceptionStr(isolate, exception);
    v8::String::Utf8Value script_name(isolate,
                                      message->GetScriptResourceName());
    v8::String::Utf8Value line_str(isolate, line);
    v8::String::Utf8Value col_str(isolate, column);
    snprintf(buf, sizeof(buf), "%s\n%s %s:%s\n", *exceptionStr,
             *script_name, *line_str, *col_str);
    *exception_str += buf;
  }
}

void HandleException(v8::Local<v8::Context> context,
                     v8::Local<v8::Value> exception)
{
  v8::Isolate *isolate = context->GetIsolate();
  js_runtime *rt = FromIsolate(isolate);
  std::string exception_str;
  HandleExceptionStr(context, exception, &exception_str);
  if (rt != nullptr)
  {
    printf("last exception: %s\n", exception_str.c_str());
    rt->last_exception = exception_str;
  }
  else
  {
    printf("Pre-Fly Exception %s\n", exception_str.c_str());
    exit(1);
  }
}

void GetNextStreamId(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  args.GetReturnValue().Set(v8::Number::New(isolate, c_get_next_stream_id()));
}

void Print(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  // TODO: assert arguments (level:Number, msg:String)
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = static_cast<js_runtime *>(isolate->GetData(0));

  auto lvl = v8::Int32::Cast(*args[0])->Value();

  v8::HandleScope handle_scope(isolate);
  v8::String::Utf8Value msg(isolate, args[1]);

  rt->print_cb(rt, lvl, *msg);
}

static v8::Local<v8::Value> ImportBuf(const js_runtime *rt, fly_buf buf)
{
  if (buf.alloc_ptr == nullptr)
  {
    // If alloc_ptr isn't set, we memcpy.
    // This is currently used for flatbuffers created in Rust.

    if (!rt->allocator->Check(buf.data_len))
    {
      return rt->isolate->ThrowException(v8::Exception::RangeError(v8_str("ArrayBuffer allocation failed.")));
    }
    auto ab = v8::ArrayBuffer::New(rt->isolate, buf.data_len);
    memcpy(ab->GetContents().Data(), buf.data_ptr, buf.data_len);
    auto view = v8::Uint8Array::New(ab, 0, buf.data_len);
    return view;
  }
  else
  {
    auto ab = v8::ArrayBuffer::New(
        rt->isolate, reinterpret_cast<void *>(buf.alloc_ptr), buf.alloc_len,
        v8::ArrayBufferCreationMode::kInternalized);
    auto view =
        v8::Uint8Array::New(ab, buf.data_ptr - buf.alloc_ptr, buf.data_len);
    return view;
  }
}

static fly_buf GetContents(const js_runtime *rt,
                           v8::Local<v8::ArrayBufferView> view)
{

  auto ab = view->Buffer();
  auto contents = ab->GetContents();

  auto length = view->ByteLength();

  fly_buf buf;
  buf.alloc_ptr = reinterpret_cast<uint8_t *>(contents.Data());
  buf.alloc_len = contents.ByteLength();
  buf.data_ptr = buf.alloc_ptr + view->ByteOffset();
  buf.data_len = length;

  return buf;
}

bool ExecuteV8StringSource(v8::Local<v8::Context> context,
                           const char *filename,
                           const char *code)
{
  auto *isolate = context->GetIsolate();
  v8::Isolate::Scope isolate_scope(isolate);
  v8::HandleScope handle_scope(isolate);

  v8::Context::Scope context_scope(context);

  v8::TryCatch try_catch(isolate);

  auto name = v8_str(filename);

  v8::ScriptOrigin origin(name);

  auto script = v8::Script::Compile(context, v8_str(code), &origin);

  if (script.IsEmpty())
  {
    // DCHECK(try_catch.HasCaught());
    HandleException(context, try_catch.Exception());
    return false;
  }

  auto result = script.ToLocalChecked()->Run(context);

  if (result.IsEmpty())
  {
    // DCHECK(try_catch.HasCaught());
    HandleException(context, try_catch.Exception());
    return false;
  }

  return true;
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
    isolate->ThrowException(v8_str("libdeno.recv already called."));
    return;
  }

  v8::Local<v8::Value> v = args[0];
  // CHECK(v->IsFunction());
  v8::Local<v8::Function> func = v8::Local<v8::Function>::Cast(v);

  rt->recv.Reset(isolate, func);
}

void Send(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = static_cast<js_runtime *>(isolate->GetData(0));
  // DCHECK_EQ(d->isolate, isolate);

  v8::Locker locker(rt->isolate);
  v8::EscapableHandleScope handle_scope(isolate);

  // CHECK_EQ(args.Length(), 1);
  v8::Local<v8::Value> ab_v = args[0];
  // CHECK(ab_v->IsArrayBufferView());
  auto buf = GetContents(rt, v8::Local<v8::ArrayBufferView>::Cast(ab_v));

  // DCHECK_EQ(d->current_args, nullptr);

  fly_buf raw = fly_buf{0, 0, 0, 0};
  if (args[1]->IsArrayBufferView())
  {
    raw = GetContents(rt, v8::Local<v8::ArrayBufferView>::Cast(args[1]));
  }

  rt->current_args = &args;
  rt->recv_cb(rt, buf, raw);
  rt->current_args = nullptr;
}

void SetGlobalErrorHandler(const v8::FunctionCallbackInfo<v8::Value> &args)
{
  v8::Isolate *isolate = args.GetIsolate();
  js_runtime *rt = FromIsolate(args.GetIsolate());
  // DCHECK_EQ(d->isolate, isolate);

  v8::HandleScope handle_scope(isolate);

  if (!rt->global_error_handler.IsEmpty())
  {
    isolate->ThrowException(
        v8_str("libfly.setGlobalErrorHandler already called."));
    return;
  }

  v8::Local<v8::Value> v = args[0];
  // CHECK(v->IsFunction());
  v8::Local<v8::Function> func = v8::Local<v8::Function>::Cast(v);

  rt->global_error_handler.Reset(isolate, func);
}

intptr_t ext_refs[] = {
    reinterpret_cast<intptr_t>(Print),
    reinterpret_cast<intptr_t>(Send),
    reinterpret_cast<intptr_t>(Recv),
    reinterpret_cast<intptr_t>(SetGlobalErrorHandler),
    reinterpret_cast<intptr_t>(GetNextStreamId),
    0};

void InitContext(v8::Isolate *isolate, v8::Local<v8::Context> context)
{
  v8::HandleScope handle_scope(isolate);
  v8::Context::Scope context_scope(context);

  auto fly = v8::Object::New(isolate);
  context->Global()->Set(context, v8_str(isolate, "libfly"), fly).FromJust();

  auto print_tmpl = v8::FunctionTemplate::New(isolate, Print);
  auto print_val = print_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "print"), print_val).FromJust();

  auto send_tmpl = v8::FunctionTemplate::New(isolate, Send);
  auto send_val = send_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "send"), send_val).FromJust();

  auto recv_tmpl = v8::FunctionTemplate::New(isolate, Recv);
  auto recv_val = recv_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "recv"), recv_val).FromJust();

  auto ge_tmpl = v8::FunctionTemplate::New(isolate, SetGlobalErrorHandler);
  auto ge_val = ge_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "setGlobalErrorHandler"), ge_val).FromJust();

  auto gnsi_tmpl = v8::FunctionTemplate::New(isolate, GetNextStreamId);
  auto gnsi_val = gnsi_tmpl->GetFunction(context).ToLocalChecked();
  fly->Set(context, v8_str(isolate, "getNextStreamId"), gnsi_val).FromJust();
}

extern "C"
{
  const char *js_version()
  {
    return v8::V8::GetVersion();
  }

  void js_init()
  {
    auto p = v8::platform::CreateDefaultPlatform();
    v8::V8::InitializePlatform(p);
    v8::V8::Initialize();
    return;
  }

  // void PromiseRejectCallback(v8::PromiseRejectMessage promise_reject_message)
  // {

  // }

  void log_event_cb(const char *name, int event)
  {
    // printf("event cb, name: %s, event: %i\n", name, event);
  }

  void microtasks_completed_cb(v8::Isolate *isolate)
  {
    // printf("microtasks completed!\n");
  }

  void promise_rejected_cb(v8::PromiseRejectMessage message)
  {
    printf("Unhandled promise rejection:\n");
    auto iso = v8::Isolate::GetCurrent();
    auto rt = FromIsolate(iso);
    v8::HandleScope handle_scope(rt->isolate);
    auto error = message.GetValue();
    auto context = rt->context.Get(rt->isolate);

    v8::Context::Scope context_scope(context);

    switch (message.GetEvent())
    {
    case v8::kPromiseRejectWithNoHandler:
      printf("no handler!\n");
      printf("is native error? %s\n", error->IsNativeError() ? "true" : "false");
      printf("%s\n", *v8::String::Utf8Value(v8::Isolate::GetCurrent(), error));
      break;
    case v8::kPromiseRejectAfterResolved:
      printf("promise reject after resolved\n");
      break;
    case v8::kPromiseHandlerAddedAfterReject:
      printf("promise handler added after reject\n");
      break;
    case v8::kPromiseResolveAfterResolved:
      printf("promise resolved after resolved\n");
      break;
    }
  }

  const js_runtime *js_runtime_new(js_runtime_options options)
  {
    js_runtime *rt = new js_runtime;

    rt->recv_cb = options.recv_cb;
    rt->print_cb = options.print_cb;
    rt->allocator = new LimitedAllocator(options.soft_memory_limit * 1024 * 1024, options.hard_memory_limit * 1024 * 1024);
    v8::Isolate::CreateParams params;

    params.array_buffer_allocator = rt->allocator;

    // if (snapshot.len > 0)
    // {
    auto *blob = new v8::StartupData;
    blob->data = options.snapshot.ptr;
    blob->raw_size = static_cast<int>(options.snapshot.len);
    params.snapshot_blob = blob;
    params.external_references = ext_refs;
    // }

    v8::ResourceConstraints rc;
    rc.set_max_old_space_size(options.soft_memory_limit);

    params.constraints = rc;

    v8::Isolate *isolate = v8::Isolate::New(params);
    // isolate->SetPromiseRejectCallback(PromiseRejectCallback);
    isolate->SetData(0, rt);
    rt->isolate = isolate;

    isolate->SetEventLogger(log_event_cb);
    isolate->AddMicrotasksCompletedCallback(microtasks_completed_cb);
    isolate->SetPromiseRejectCallback(promise_rejected_cb);

    v8::Locker locker(isolate);
    v8::Isolate::Scope isolate_scope(isolate);
    {
      v8::HandleScope handle_scope(isolate);
      auto context = v8::Context::New(isolate, nullptr, v8::MaybeLocal<v8::ObjectTemplate>(), v8::MaybeLocal<v8::Value>(), v8::DeserializeInternalFieldsCallback(DeserializeInternalFields, nullptr));

      InitContext(isolate, context);

      rt->context.Reset(rt->isolate, context);
    }

    rt->data = options.data;

    delete blob;

    return rt;
  }

  const void *js_get_data(const js_runtime *rt)
  {
    return rt->data;
  }

  int js_send(const js_runtime *rt, fly_buf buf, fly_buf raw)
  {
    v8::Locker locker(rt->isolate);
    v8::Isolate::Scope isolate_scope(rt->isolate);
    v8::HandleScope handle_scope(rt->isolate);

    auto context = rt->context.Get(rt->isolate);
    v8::Context::Scope context_scope(context);

    v8::TryCatch try_catch(rt->isolate);
    try_catch.SetVerbose(true);

    auto recv = rt->recv.Get(rt->isolate);
    if (recv.IsEmpty())
    {
      // rt->last_exception = "libdeno.recv has not been called.";
      printf("libfly.recv has not been called\n");
      return 0;
    }

    auto args_len = raw.data_len > 0 ? 2 : 1;
    auto *args = new v8::Local<v8::Value>[args_len];

    args[0] = ImportBuf(rt, buf);
    free_fly_buf(buf);
    if (raw.data_len > 0)
    {
      args[1] = ImportBuf(rt, raw);
      // free_fly_buf(raw);
    }

    recv->Call(context->Global(), args_len, args);

    delete[] args;

    if (try_catch.HasCaught())
    {
      HandleException(context, try_catch.Exception());
      return 0;
    }

    return 1;
  }

  void js_set_response(const js_runtime *rt, fly_buf buf)
  {
    auto ab = ImportBuf(rt, buf);
    free_fly_buf(buf);
    rt->current_args->GetReturnValue().Set(ab);
  }

  void js_runtime_terminate(js_runtime *rt)
  {
    rt->context.Reset();
    rt->isolate->Dispose();
    free(rt);
  }

  void js_runtime_run_micro_tasks(const js_runtime *rt)
  {
    rt->isolate->RunMicrotasks();
  }

  bool js_eval(const js_runtime *rt, const char *filename, const char *code)
  {
    VALUE_SCOPE(rt->isolate, rt->context);
    return ExecuteV8StringSource(ctx, filename, code);
    // v8::TryCatch try_catch(rt->isolate);
    // try_catch.SetVerbose(true);

    // v8::ScriptOrigin origin = v8::ScriptOrigin(v8_str(rt->isolate, filename));
    // v8::MaybeLocal<v8::Script> script = v8::Script::Compile(
    //     ctx,
    //     v8_str(rt->isolate, code),
    //     &origin);

    // if (script.IsEmpty())
    // {
    //   printf("errrrr compiling!\n");
    //   return;
    // }

    // v8::MaybeLocal<v8::Value> result = script.ToLocalChecked()->Run(ctx);
    // if (result.IsEmpty())
    // {
    //   printf("errrrr evaluating!\n");
    //   return;
    // }
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
    {
      v8::Locker locker(isolate);
      isolate->LowMemoryNotification();
    }
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
        rt->allocator->allocated,
    };
  }

  bool js_dump_heap_snapshot(const js_runtime *rt, const char *filename)
  {
    FILE *fp = fopen(filename, "w");
    if (fp == NULL)
      return false;
    auto hp = rt->isolate->GetHeapProfiler();
    const v8::HeapSnapshot *const snap = hp->TakeHeapSnapshot();
    FileOutputStream stream(fp);
    snap->Serialize(&stream, HeapSnapshot::kJSON);
    fclose(fp);
    // Work around a deficiency in the API.  The HeapSnapshot object is const
    // but we cannot call HeapProfiler::DeleteAllHeapSnapshots() because that
    // invalidates _all_ snapshots, including those created by other tools.
    const_cast<HeapSnapshot *>(snap)->Delete();
    return true;
  }

  void js_runtime_dispose(const runtime *rt)
  {
    rt->isolate->Dispose();
    delete rt;
  }

  fly_simple_buf js_create_snapshot(const char *filename, const char *code)
  {
    v8::StartupData blob;
    {
      v8::SnapshotCreator creator(ext_refs);
      v8::Isolate *isolate = creator.GetIsolate();
      {
        v8::HandleScope handle_scope(isolate);
        v8::Local<v8::Context> context = v8::Context::New(isolate);

        v8::Context::Scope context_scope(context);

        InitContext(isolate, context);
        ExecuteV8StringSource(context, filename, code);

        creator.SetDefaultContext(context, v8::SerializeInternalFieldsCallback(
                                               SerializeInternalFields, nullptr));
      }
      blob =
          creator.CreateBlob(v8::SnapshotCreator::FunctionCodeHandling::kClear);
    }

    return fly_simple_buf{blob.data, blob.raw_size};
  }
}
