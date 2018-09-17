#ifndef libfly
#define libfly

/* Generated with cbindgen:0.6.3 */

// Auto-generated, don't edit!

#include <cstdint>
#include <cstdlib>
#include "runtime.h"

enum class MessageKind {
  TimerStart,
  TimerReady,
};

struct fly_buf {
  const char *ptr;
  size_t len;

  bool operator==(const fly_buf& other) const {
    return ptr == other.ptr &&
           len == other.len;
  }
};

struct js_heap_stats {
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

  bool operator==(const js_heap_stats& other) const {
    return total_heap_size == other.total_heap_size &&
           total_heap_size_executable == other.total_heap_size_executable &&
           total_physical_size == other.total_physical_size &&
           total_available_size == other.total_available_size &&
           used_heap_size == other.used_heap_size &&
           heap_size_limit == other.heap_size_limit &&
           malloced_memory == other.malloced_memory &&
           peak_malloced_memory == other.peak_malloced_memory &&
           number_of_native_contexts == other.number_of_native_contexts &&
           number_of_detached_contexts == other.number_of_detached_contexts &&
           does_zap_garbage == other.does_zap_garbage;
  }
};

struct KeyValue {
  const char *key;
  const Value *val;

  bool operator==(const KeyValue& other) const {
    return key == other.key &&
           val == other.val;
  }
};

struct fly_bytes {
  uint8_t *alloc_ptr;
  uintptr_t alloc_len;
  uint8_t *data_ptr;
  uintptr_t data_len;

  bool operator==(const fly_bytes& other) const {
    return alloc_ptr == other.alloc_ptr &&
           alloc_len == other.alloc_len &&
           data_ptr == other.data_ptr &&
           data_len == other.data_len;
  }
};

struct Value {
  enum class Tag : uint8_t {
    None,
    Int32,
    String,
    Object,
    Array,
    ArrayBuffer,
    Uint8Array,
  };

  struct Int32_Body {
    int32_t _0;

    bool operator==(const Int32_Body& other) const {
      return _0 == other._0;
    }
  };

  struct String_Body {
    const char *_0;

    bool operator==(const String_Body& other) const {
      return _0 == other._0;
    }
  };

  struct Object_Body {
    int32_t len;
    const KeyValue *pairs;

    bool operator==(const Object_Body& other) const {
      return len == other.len &&
             pairs == other.pairs;
    }
  };

  struct Array_Body {
    size_t len;
    const Value *values;

    bool operator==(const Array_Body& other) const {
      return len == other.len &&
             values == other.values;
    }
  };

  struct ArrayBuffer_Body {
    fly_buf _0;

    bool operator==(const ArrayBuffer_Body& other) const {
      return _0 == other._0;
    }
  };

  struct Uint8Array_Body {
    fly_bytes _0;

    bool operator==(const Uint8Array_Body& other) const {
      return _0 == other._0;
    }
  };

  Tag tag;
  union {
    Int32_Body int32;
    String_Body string;
    Object_Body object;
    Array_Body array;
    ArrayBuffer_Body array_buffer;
    Uint8Array_Body uint8_array;
  };

  static Value None() {
    Value result;
    result.tag = Tag::None;
    return result;
  }

  static Value Int32(int32_t const& a0) {
    Value result;
    result.int32._0 = a0;
    result.tag = Tag::Int32;
    return result;
  }

  static Value String(const char *const& a0) {
    Value result;
    result.string._0 = a0;
    result.tag = Tag::String;
    return result;
  }

  static Value Object(int32_t const& aLen,
                      const KeyValue *const& aPairs) {
    Value result;
    result.object.len = aLen;
    result.object.pairs = aPairs;
    result.tag = Tag::Object;
    return result;
  }

  static Value Array(size_t const& aLen,
                     const Value *const& aValues) {
    Value result;
    result.array.len = aLen;
    result.array.values = aValues;
    result.tag = Tag::Array;
    return result;
  }

  static Value ArrayBuffer(fly_buf const& a0) {
    Value result;
    result.array_buffer._0 = a0;
    result.tag = Tag::ArrayBuffer;
    return result;
  }

  static Value Uint8Array(fly_bytes const& a0) {
    Value result;
    result.uint8_array._0 = a0;
    result.tag = Tag::Uint8Array;
    return result;
  }

  bool IsNone() const {
    return tag == Tag::None;
  }

  bool IsInt32() const {
    return tag == Tag::Int32;
  }

  bool IsString() const {
    return tag == Tag::String;
  }

  bool IsObject() const {
    return tag == Tag::Object;
  }

  bool IsArray() const {
    return tag == Tag::Array;
  }

  bool IsArrayBuffer() const {
    return tag == Tag::ArrayBuffer;
  }

  bool IsUint8Array() const {
    return tag == Tag::Uint8Array;
  }

  bool operator==(const Value& other) const {
    if (tag != other.tag) {
      return false;
    }
    switch (tag) {
      case Tag::Int32: return int32 == other.int32;
      case Tag::String: return string == other.string;
      case Tag::Object: return object == other.object;
      case Tag::Array: return array == other.array;
      case Tag::ArrayBuffer: return array_buffer == other.array_buffer;
      case Tag::Uint8Array: return uint8_array == other.uint8_array;
      default: return true;
    }
  }
};

struct Message {
  uint32_t id;
  bool sync;
  MessageKind kind;
  Value value;

  bool operator==(const Message& other) const {
    return id == other.id &&
           sync == other.sync &&
           kind == other.kind &&
           value == other.value;
  }
};

extern "C" {

extern fly_buf js_create_snapshot(const char *filename, const char *code);

extern void js_eval(const js_runtime *rt, const char *filename, const char *code);

extern const void *js_get_data(const js_runtime *rt);

extern void js_init(fly_buf natives, fly_buf snapshot);

extern js_heap_stats js_runtime_heap_statistics(const js_runtime *rt);

extern const js_runtime *js_runtime_new(fly_buf snapshot, void *data);

extern Value js_send(const js_runtime *rt, MessageKind kind, Value cmd);

extern void js_set_response(const js_runtime *rt, int cmd_id, int argc, const Value *argv);

extern void js_set_return_value(const js_runtime *rt, Value v);

extern const char *js_version();

extern Value testy(const js_runtime *rt, Message msg);

} // extern "C"

#endif // libfly
