/* Cold whole-document MD4C baseline with no-op callbacks.
 *
 * This is intentionally not an incremental design. It exists only to put the
 * donor's current flat-input/full-finalization cost next to the block-seam
 * measurements.
 */

#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "md4c.h"

static uint64_t
now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t) ts.tv_sec * UINT64_C(1000000000) + (uint64_t) ts.tv_nsec;
}

static int
enter_block(MD_BLOCKTYPE type, void* detail, void* userdata)
{
    (void) type;
    (void) detail;
    (void) userdata;
    return 0;
}

static int
leave_block(MD_BLOCKTYPE type, void* detail, void* userdata)
{
    (void) type;
    (void) detail;
    (void) userdata;
    return 0;
}

static int
enter_span(MD_SPANTYPE type, void* detail, void* userdata)
{
    (void) type;
    (void) detail;
    (void) userdata;
    return 0;
}

static int
leave_span(MD_SPANTYPE type, void* detail, void* userdata)
{
    (void) type;
    (void) detail;
    (void) userdata;
    return 0;
}

static int
text_callback(MD_TEXTTYPE type, const MD_CHAR* text, MD_SIZE size, void* userdata)
{
    (void) type;
    (void) text;
    (void) size;
    (void) userdata;
    return 0;
}

static char*
read_stdin(MD_SIZE* out_size)
{
    size_t used = 0;
    size_t capacity = 4096;
    char* text = (char*) malloc(capacity);
    if(text == NULL)
        return NULL;
    for(;;) {
        size_t n;
        if(used == capacity) {
            char* grown;
            capacity += capacity / 2;
            grown = (char*) realloc(text, capacity);
            if(grown == NULL) {
                free(text);
                return NULL;
            }
            text = grown;
        }
        n = fread(text + used, 1, capacity - used, stdin);
        used += n;
        if(n == 0) {
            if(ferror(stdin)) {
                free(text);
                return NULL;
            }
            break;
        }
    }
    if(used > UINT_MAX) {
        free(text);
        errno = EFBIG;
        return NULL;
    }
    *out_size = (MD_SIZE) used;
    return text;
}

int
main(void)
{
    MD_PARSER parser = { 0 };
    MD_SIZE size;
    char* source = read_stdin(&size);
    uint64_t started;
    uint64_t elapsed;
    int ret;
    if(source == NULL) {
        fprintf(stderr, "read failed: %s\n", strerror(errno));
        return 2;
    }
    parser.abi_version = 0;
    parser.flags = MD_FLAG_PERMISSIVEAUTOLINKS |
                   MD_FLAG_TABLES |
                   MD_FLAG_STRIKETHROUGH |
                   MD_FLAG_TASKLISTS;
    parser.enter_block = enter_block;
    parser.leave_block = leave_block;
    parser.enter_span = enter_span;
    parser.leave_span = leave_span;
    parser.text = text_callback;
    started = now_ns();
    ret = md_parse(source, size, &parser, NULL);
    elapsed = now_ns() - started;
    printf("summary bytes=%u ret=%d elapsed_ns=%" PRIu64 "\n",
           size, ret, elapsed);
    free(source);
    return ret == 0 ? 0 : 1;
}
