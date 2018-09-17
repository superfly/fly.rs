#ifndef libfly
#define libfly

/* Generated with cbindgen:0.6.3 */

// Auto-generated, don't edit!

#include <cstdint>
#include <cstdlib>

struct js_runtime {
  uint8_t _unused[0];
};

struct fly_buf {
  const uint8_t *ptr;
  uintptr_t len;
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
};

struct fly_bytes {
  uint8_t *alloc_ptr;
  uintptr_t alloc_len;
  uint8_t *data_ptr;
  uintptr_t data_len;
};

struct KeyValue {
  const char *key;
  const Value *val;
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
  };

  struct String_Body {
    const char *_0;
  };

  struct Object_Body {
    int32_t len;
    const KeyValue *pairs;
  };

  struct Array_Body {
    size_t len;
    const Value *values;
  };

  struct ArrayBuffer_Body {
    fly_buf _0;
  };

  struct Uint8Array_Body {
    fly_bytes _0;
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
};

extern "C" {

extern void js_eval(const js_runtime *rt, const char *filename, const char *code);

extern const void *js_get_data(const js_runtime *rt);

extern void js_init(fly_buf natives, fly_buf snapshot);

extern js_heap_stats js_runtime_heap_statistics(const js_runtime *rt);

extern const js_runtime *js_runtime_new(const void *data);

extern int js_send(const js_runtime *rt, fly_bytes buf);

extern void js_set_response(const js_runtime *rt, int cmd_id, int argc, const Value *argv);

extern void js_set_return_value(const js_runtime *rt, Value v);

extern fly_buf js_snapshot_create(const char *s);

extern const char *js_version();

extern Value testy(const js_runtime *rt, int id, const char *name, int argc, const Value *argv);

} // extern "C"

#endif // libfly
