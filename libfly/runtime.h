#include <v8.h>

// forward declaration
struct Value;

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
}