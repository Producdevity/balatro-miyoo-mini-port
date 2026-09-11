#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stddef.h>

#define ENTRY_COUNT 1024

struct copy_entry {
    uintptr_t caller;
    uint64_t calls;
    uint64_t bytes;
    unsigned move;
};

static struct copy_entry entries[ENTRY_COUNT];
static uint64_t dropped;
static atomic_flag owned = ATOMIC_FLAG_INIT;
static atomic_int active;
static _Thread_local int enabled __attribute__((tls_model("local-exec")));

void *__real_memcpy(void *dest, const void *src, size_t size);
void *__real_memmove(void *dest, const void *src, size_t size);

static void record_copy(uintptr_t caller, size_t size, unsigned move) {
    // libc copies during TLS initialization, before thread-local access is valid.
    if (!atomic_load_explicit(&active, memory_order_relaxed)) return;
    if (!enabled) return;
    size_t slot = ((caller >> 2) ^ move) % ENTRY_COUNT;
    for (size_t i = 0; i < ENTRY_COUNT; ++i) {
        struct copy_entry *entry = &entries[(slot + i) % ENTRY_COUNT];
        if (!entry->calls || (entry->caller == caller && entry->move == move)) {
            if (!entry->calls) entry->bytes = 0;
            entry->caller = caller;
            entry->move = move;
            entry->calls++;
            entry->bytes += size;
            return;
        }
    }
    dropped++;
}

__attribute__((noinline))
void *__wrap_memcpy(void *dest, const void *src, size_t size) {
    record_copy((uintptr_t)__builtin_extract_return_addr(__builtin_return_address(0)), size, 0);
    return __real_memcpy(dest, src, size);
}

__attribute__((noinline))
void *__wrap_memmove(void *dest, const void *src, size_t size) {
    record_copy((uintptr_t)__builtin_extract_return_addr(__builtin_return_address(0)), size, 1);
    return __real_memmove(dest, src, size);
}

int balatro_copy_trace_begin(void) {
    if (atomic_flag_test_and_set(&owned)) return 0;
    for (size_t i = 0; i < ENTRY_COUNT; ++i) entries[i].calls = 0;
    dropped = 0;
    enabled = 1;
    atomic_store_explicit(&active, 1, memory_order_relaxed);
    return 1;
}

void balatro_copy_trace_end(void) {
    atomic_store_explicit(&active, 0, memory_order_relaxed);
    enabled = 0;
    fprintf(stderr, "[copy-trace] thread=main dropped=%llu\n", (unsigned long long)dropped);
    for (size_t i = 0; i < ENTRY_COUNT; ++i) {
        const struct copy_entry *entry = &entries[i];
        if (entry->calls) {
            fprintf(stderr, "[copy-trace] op=%s caller=0x%lx calls=%llu bytes=%llu\n",
                    entry->move ? "memmove" : "memcpy", (unsigned long)entry->caller,
                    (unsigned long long)entry->calls, (unsigned long long)entry->bytes);
        }
    }
    atomic_flag_clear(&owned);
}
