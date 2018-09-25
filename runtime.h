#include "allocator.h"
#include <v8.h>
#include <string>
#include <sstream>
#include <iostream>

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
    v8::Persistent<v8::Function> global_error_handler;
    const v8::FunctionCallbackInfo<v8::Value> *current_args;
    LimitedAllocator *allocator;
    std::string last_exception;
  } js_runtime;
}