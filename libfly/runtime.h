#ifndef INTERNAL_H_
#define INTERNAL_H_

#include "allocator.h"
#include <v8.h>
#include <string>
#include <sstream>
#include <iostream>

extern "C"
{
  struct js_runtime
  {
    v8::Isolate *isolate;
    v8::Persistent<v8::Context> context;
    void *data;
    v8::Persistent<v8::Function> recv;
    v8::Persistent<v8::Function> global_error_handler;
    const v8::FunctionCallbackInfo<v8::Value> *current_args;
    LimitedAllocator *allocator;
    fly_recv_cb recv_cb;
    fly_print_cb print_cb;
    std::string last_exception;
  };
}

#endif // INTERNAL_H_