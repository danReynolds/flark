/*
 * Disposable MD4C block-phase probe.
 *
 * This deliberately includes MD4C's implementation so the experiment can
 * exercise the private block seam without pretending MD4C exposes a supported
 * block-parser API. Compile with an include path pointing at the pinned MD4C
 * src directory:
 *
 *   cc -O2 -std=c11 -I /tmp/flark-md4c-gate/src block_probe.c -o block_probe
 */

#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <inttypes.h>
#include <time.h>

#include "md4c.c"

typedef struct PROBE_RESULT_tag {
    MD_SIZE bytes;
    MD_SIZE lines;
    MD_SIZE max_line_bytes;
    MD_SIZE leaf_blocks;
    MD_SIZE container_records;
    MD_SIZE leaf_lines;
    MD_SIZE source_runs;
    MD_SIZE refs;
    MD_SIZE table_blocks;
    MD_SIZE html_blocks;
    MD_SIZE list_records;
    MD_SIZE max_container_depth;
    uint64_t elapsed_ns;
    uint64_t max_step_ns;
    int canceled;
    int ret;
} PROBE_RESULT;

static uint64_t
probe_now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t) ts.tv_sec * UINT64_C(1000000000) + (uint64_t) ts.tv_nsec;
}

static const char*
probe_block_name(MD_BLOCKTYPE type)
{
    switch(type) {
        case MD_BLOCK_DOC: return "doc";
        case MD_BLOCK_QUOTE: return "quote";
        case MD_BLOCK_UL: return "ul";
        case MD_BLOCK_OL: return "ol";
        case MD_BLOCK_LI: return "li";
        case MD_BLOCK_HR: return "hr";
        case MD_BLOCK_H: return "heading";
        case MD_BLOCK_CODE: return "code";
        case MD_BLOCK_HTML: return "html";
        case MD_BLOCK_P: return "paragraph";
        case MD_BLOCK_TABLE: return "table";
        case MD_BLOCK_ADMONITION: return "admonition";
        default: return "other";
    }
}

static void
probe_init_ctx(MD_CTX* ctx, const char* text, MD_SIZE size)
{
    int i;
    memset(ctx, 0, sizeof(*ctx));
    ctx->text = text;
    ctx->size = size;
    ctx->parser.abi_version = 0;
    /* Match Flark's current GFM feature profile; exclude MD4C-only footnotes
     * and admonitions even though MD_DIALECT_GITHUB now enables them. */
    ctx->parser.flags = MD_FLAG_PERMISSIVEAUTOLINKS |
                        MD_FLAG_TABLES |
                        MD_FLAG_STRIKETHROUGH |
                        MD_FLAG_TASKLISTS;
    ctx->code_indent_offset = 4;
    ctx->doc_ends_with_newline =
        (size > 0 && ISNEWLINE_(text[size - 1]));
    ctx->ref_def_hashtable.def_size = sizeof(MD_REF_DEF);
    ctx->max_ref_def_output = 16 * MIN(size, (MD_SIZE) (1024 * 1024 / 16));
    ctx->footnote_hashtable.def_size = sizeof(MD_FOOTNOTE_DEF);
    for(i = 0; i < (int) SIZEOF_ARRAY(ctx->opener_stacks); i++)
        ctx->opener_stacks[i].top = -1;
    ctx->ptr_stack.top = -1;
    ctx->unresolved_link_head = -1;
    ctx->unresolved_link_tail = -1;
    ctx->table_cell_boundaries_head = -1;
    ctx->table_cell_boundaries_tail = -1;
}

static void
probe_free_ctx(MD_CTX* ctx)
{
    md_free_ref_defs(ctx);
    md_free_footnote_defs(ctx);
    free(ctx->buffer);
    free(ctx->marks);
    free(ctx->block_bytes);
    free(ctx->containers);
}

static void
probe_measure_tape(MD_CTX* ctx, PROBE_RESULT* result, int dump)
{
    int byte_off = 0;
    while(byte_off < ctx->n_block_bytes) {
        MD_BLOCK* block = (MD_BLOCK*) ((char*) ctx->block_bytes + byte_off);
        int is_container = (block->flags & MD_BLOCK_CONTAINER) != 0;
        if(is_container) {
            result->container_records++;
            if(block->type == MD_BLOCK_UL || block->type == MD_BLOCK_OL ||
               block->type == MD_BLOCK_LI)
                result->list_records++;
            if(dump) {
                printf("container type=%s flags=%u data=%u value=%u\n",
                       probe_block_name(block->type), block->flags,
                       block->data, block->n_lines);
            }
        } else {
            MD_SIZE i;
            result->leaf_blocks++;
            result->leaf_lines += block->n_lines;
            result->source_runs += block->n_lines;
            if(block->type == MD_BLOCK_TABLE)
                result->table_blocks++;
            if(block->type == MD_BLOCK_HTML)
                result->html_blocks++;
            if(dump) {
                printf("leaf type=%s flags=%u data=%u lines=%u",
                       probe_block_name(block->type), block->flags,
                       block->data, block->n_lines);
                if(block->n_lines > 0) {
                    if(block->type == MD_BLOCK_CODE || block->type == MD_BLOCK_HTML) {
                        MD_VERBATIMLINE* lines = (MD_VERBATIMLINE*) (block + 1);
                        printf(" origin=%u..%u", lines[0].beg,
                               lines[block->n_lines - 1].end);
                    } else {
                        MD_LINE* lines = (MD_LINE*) (block + 1);
                        printf(" origin=%u..%u", lines[0].beg,
                               lines[block->n_lines - 1].end);
                    }
                }
                printf("\n");
            }
            if(block->type == MD_BLOCK_CODE || block->type == MD_BLOCK_HTML)
                byte_off += block->n_lines * (int) sizeof(MD_VERBATIMLINE);
            else
                byte_off += block->n_lines * (int) sizeof(MD_LINE);
            for(i = 0; i < block->n_lines; i++) {
                /* Each retained physical line is one origin run. This is
                 * intentionally counted rather than coalesced: it is MD4C's
                 * actual block representation. */
            }
        }
        byte_off += (int) sizeof(MD_BLOCK);
    }
}

static PROBE_RESULT
probe_block_phase(const char* text, MD_SIZE size, MD_SIZE cancel_after_lines,
                  int dump)
{
    MD_CTX ctx;
    const MD_LINE_ANALYSIS* pivot_line = &md_dummy_blank_line;
    MD_LINE_ANALYSIS line_buf[2];
    MD_LINE_ANALYSIS* line = &line_buf[0];
    OFF off = 0;
    PROBE_RESULT result = { 0 };
    uint64_t started = probe_now_ns();

    result.bytes = size;
    probe_init_ctx(&ctx, text, size);

    while(off < ctx.size) {
        OFF line_start = off;
        uint64_t step_started;
        uint64_t step_ns;
        if(cancel_after_lines > 0 && result.lines >= cancel_after_lines) {
            result.canceled = 1;
            break;
        }
        if(line == pivot_line)
            line = (line == &line_buf[0] ? &line_buf[1] : &line_buf[0]);
        step_started = probe_now_ns();
        result.ret = md_analyze_line(&ctx, off, &off, pivot_line, line);
        if(result.ret == 0)
            result.ret = md_process_line(&ctx, &pivot_line, line);
        step_ns = probe_now_ns() - step_started;
        if(step_ns > result.max_step_ns)
            result.max_step_ns = step_ns;
        if(result.ret != 0)
            break;
        result.lines++;
        if((MD_SIZE) (off - line_start) > result.max_line_bytes)
            result.max_line_bytes = off - line_start;
        if((MD_SIZE) ctx.n_containers > result.max_container_depth)
            result.max_container_depth = ctx.n_containers;
    }

    if(result.ret == 0 && !result.canceled) {
        result.ret = md_end_current_block(&ctx);
        if(result.ret == 0)
            result.ret = md_build_ref_def_hashtable(&ctx);
        if(result.ret == 0)
            result.ret = md_leave_child_containers(&ctx, 0);
    }

    result.refs = ctx.ref_def_hashtable.n_defs;
    probe_measure_tape(&ctx, &result, dump);
    result.elapsed_ns = probe_now_ns() - started;

    printf("summary bytes=%u lines=%u max_line_bytes=%u canceled=%d ret=%d "
           "block_bytes=%d block_capacity=%d container_capacity=%d "
           "leaf_blocks=%u container_records=%u leaf_lines=%u "
           "source_runs=%u refs=%u tables=%u html=%u lists=%u "
           "max_depth=%u elapsed_ns=%" PRIu64 " max_step_ns=%" PRIu64 " "
           "sizeof_ctx=%zu sizeof_block=%zu sizeof_line=%zu "
           "sizeof_container=%zu sizeof_ref=%zu\n",
           result.bytes, result.lines, result.max_line_bytes, result.canceled,
           result.ret, ctx.n_block_bytes, ctx.alloc_block_bytes,
           ctx.alloc_containers, result.leaf_blocks, result.container_records,
           result.leaf_lines, result.source_runs, result.refs,
           result.table_blocks, result.html_blocks, result.list_records,
           result.max_container_depth, result.elapsed_ns, result.max_step_ns,
           sizeof(MD_CTX), sizeof(MD_BLOCK), sizeof(MD_LINE),
           sizeof(MD_CONTAINER), sizeof(MD_REF_DEF));

    probe_free_ctx(&ctx);
    return result;
}

static char*
probe_read_stdin(MD_SIZE* out_size)
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
main(int argc, char** argv)
{
    MD_SIZE size = 0;
    MD_SIZE cancel_after_lines = 0;
    int dump = 0;
    int i;
    char* text;
    for(i = 1; i < argc; i++) {
        if(strcmp(argv[i], "--dump") == 0) {
            dump = 1;
        } else if(strcmp(argv[i], "--cancel-after-lines") == 0 && i + 1 < argc) {
            cancel_after_lines = (MD_SIZE) strtoul(argv[++i], NULL, 10);
        } else {
            fprintf(stderr, "usage: block_probe [--dump] [--cancel-after-lines N]\n");
            return 2;
        }
    }
    text = probe_read_stdin(&size);
    if(text == NULL) {
        fprintf(stderr, "read failed: %s\n", strerror(errno));
        return 2;
    }
    PROBE_RESULT result = probe_block_phase(text, size, cancel_after_lines, dump);
    free(text);
    return result.ret == 0 ? 0 : 1;
}
