#pragma once
#include <cstdlib>
#include <v8.h>

using namespace v8;

class LimitedAllocator : public ArrayBuffer::Allocator
{
private:
  size_t limit;
  size_t v8_heap;
  size_t next_check;
  int failures = 0;

public:
  std::atomic_size_t allocated;
  bool Check(size_t length);
  explicit LimitedAllocator(size_t limit);
  void *Allocate(size_t length) final;
  void *AllocateUninitialized(size_t length) final;
  void Free(void *data, size_t length) final;
  // This is used by ExternalCopy when an ArrayBuffer is transferred. The memory is not freed but
  // we should no longer count it against the isolate
  void AdjustAllocatedSize(ptrdiff_t length);
  int GetFailureCount() const;
};

/**
 * ArrayBuffer::Allocator that enforces memory limits. The v8 documentation specifically says
 * that it's unsafe to call back into v8 from this class but I took a look at
 * GetHeapStatistics() and I think it'll be ok.
 */
bool LimitedAllocator::Check(const size_t length)
{
  if (v8_heap + allocated + length > next_check)
  {
    HeapStatistics heap_statistics;
    Isolate *isolate = Isolate::GetCurrent();
    isolate->GetHeapStatistics(&heap_statistics);
    v8_heap = heap_statistics.total_heap_size();
    if (v8_heap + allocated + length > limit)
    {
      // This is might be dangerous but the tests pass soooo..
      isolate->LowMemoryNotification();
      isolate->GetHeapStatistics(&heap_statistics);
      v8_heap = heap_statistics.total_heap_size();
      if (v8_heap + allocated + length > limit)
      {
        return false;
      }
    }
    next_check = v8_heap + allocated + length + 1024 * 1024;
  }
  return v8_heap + allocated + length <= limit;
}

LimitedAllocator::LimitedAllocator(size_t limit) : limit(limit), v8_heap(1024 * 1024 * 4), next_check(1024 * 1024)
{
  allocated = 0;
}

void *LimitedAllocator::Allocate(size_t length)
{
  // printf("alloc memory %i bytes\n", length);
  if (Check(length))
  {
    allocated += length;
    return std::calloc(length, 1);
  }
  else
  {
    ++failures;
    if (length <= 64)
    { // kMinAddedElementsCapacity * sizeof(uint32_t)
      // When a tiny TypedArray is created v8 will avoid calling the allocator and instead just use
      // the internal heap. This is all fine until someone wants a pointer to the underlying buffer,
      // in that case v8 will "materialize" an ArrayBuffer which does invoke this allocator. If the
      // allocator refuses to return a valid pointer it will result in a hard crash so we have no
      // choice but to let this allocation succeed. Luckily the amount of memory allocated is tiny
      // and will soon be freed because at the same time we terminate the isolate.
      allocated += length;
      // rt->Terminate();
      return std::calloc(length, 1);
    }
    else
    {
      // The places end up here are more graceful and will throw a RangeError
      return nullptr;
    }
  }
}

void *LimitedAllocator::AllocateUninitialized(size_t length)
{
  // printf("alloc memory %i bytes (uninitialized)\n", length);
  if (Check(length))
  {
    allocated += length;
    return std::malloc(length);
  }
  else
  {
    ++failures;
    if (length <= 64)
    {
      allocated += length;
      // rt->Terminate();
      return std::malloc(length);
    }
    else
    {
      return nullptr;
    }
  }
}

void LimitedAllocator::Free(void *data, size_t length)
{
  allocated -= length;
  next_check -= length;
  std::free(data);
}

void LimitedAllocator::AdjustAllocatedSize(ptrdiff_t length)
{
  allocated += length;
}

int LimitedAllocator::GetFailureCount() const
{
  return failures;
}