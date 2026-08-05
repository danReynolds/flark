/*
 * Deep-clone/restart experiment for MD4C's private block context.
 *
 * This intentionally demonstrates the strongest case for current MD4C state:
 * when an edit starts at a physical-line boundary and preserves the prefix, a
 * carefully rebased deep clone can resume and match a clean parse. The clone
 * still copies the accumulated prefix tape and repairs source/heap pointers;
 * it is evidence for the grammar state, not a proposed persistent checkpoint.
 */

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "md4c.c"

typedef struct PARSE_CURSOR_tag {
    OFF off;
    MD_LINE_ANALYSIS pivot;
    int pivot_is_dummy;
} PARSE_CURSOR;

static void
init_ctx(MD_CTX* ctx, const char* source, MD_SIZE size)
{
    int i;
    memset(ctx, 0, sizeof(*ctx));
    ctx->text = source;
    ctx->size = size;
    ctx->parser.abi_version = 0;
    ctx->parser.flags = MD_FLAG_PERMISSIVEAUTOLINKS |
                        MD_FLAG_TABLES |
                        MD_FLAG_STRIKETHROUGH |
                        MD_FLAG_TASKLISTS;
    ctx->code_indent_offset = 4;
    ctx->doc_ends_with_newline = size > 0 && ISNEWLINE_(source[size - 1]);
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
init_cursor(PARSE_CURSOR* cursor)
{
    memset(cursor, 0, sizeof(*cursor));
    cursor->pivot_is_dummy = TRUE;
}

static int
parse_to(MD_CTX* ctx, PARSE_CURSOR* cursor, OFF stop)
{
    int ret = 0;
    while(cursor->off < stop) {
        const MD_LINE_ANALYSIS* pivot = cursor->pivot_is_dummy
                                      ? &md_dummy_blank_line
                                      : &cursor->pivot;
        MD_LINE_ANALYSIS line;
        OFF next = cursor->off;
        MD_CHECK(md_analyze_line(ctx, cursor->off, &next, pivot, &line));
        if(next > stop) {
            fprintf(stderr, "checkpoint is not a physical-line boundary\n");
            return -2;
        }
        MD_CHECK(md_process_line(ctx, &pivot, &line));
        cursor->off = next;
        if(pivot == &md_dummy_blank_line) {
            cursor->pivot_is_dummy = TRUE;
        } else {
            cursor->pivot = *pivot;
            cursor->pivot_is_dummy = FALSE;
        }
    }
abort:
    return ret;
}

static int
finish_parse(MD_CTX* ctx, PARSE_CURSOR* cursor)
{
    int ret;
    ret = parse_to(ctx, cursor, ctx->size);
    if(ret == 0)
        ret = md_end_current_block(ctx);
    if(ret == 0)
        ret = md_build_ref_def_hashtable(ctx);
    if(ret == 0)
        ret = md_leave_child_containers(ctx, 0);
    return ret;
}

static void*
copy_bytes(const void* source, size_t bytes)
{
    void* result;
    if(bytes == 0)
        return NULL;
    result = malloc(bytes);
    if(result != NULL)
        memcpy(result, source, bytes);
    return result;
}

static const CHAR*
rebase_source_pointer(const MD_CTX* old, const MD_CTX* clone, const CHAR* ptr)
{
    ptrdiff_t offset = ptr - old->text;
    if(offset < 0 || (SZ) offset > old->size)
        return NULL;
    return clone->text + offset;
}

static int
clone_ref_defs(const MD_CTX* old, MD_CTX* clone, size_t* retained_bytes)
{
    unsigned i;
    size_t bytes = old->ref_def_hashtable.alloc_defs *
                   old->ref_def_hashtable.def_size;
    clone->ref_def_hashtable.defs = copy_bytes(old->ref_def_hashtable.defs, bytes);
    clone->ref_def_hashtable.buckets = NULL;
    clone->ref_def_hashtable.n_buckets = 0;
    if(bytes > 0 && clone->ref_def_hashtable.defs == NULL)
        return -1;
    *retained_bytes += bytes;
    for(i = 0; i < old->ref_def_hashtable.n_defs; i++) {
        const MD_REF_DEF* old_def = &old->ref_def_hashtable.ref_defs[i];
        MD_REF_DEF* new_def = &clone->ref_def_hashtable.ref_defs[i];
        if(old_def->label_needs_free) {
            new_def->entry.label = copy_bytes(old_def->entry.label,
                                              old_def->entry.label_size);
            if(new_def->entry.label == NULL && old_def->entry.label_size > 0)
                return -1;
            *retained_bytes += old_def->entry.label_size;
        } else {
            new_def->entry.label = rebase_source_pointer(old, clone,
                                                         old_def->entry.label);
            if(new_def->entry.label == NULL)
                return -1;
        }
        if(old_def->title_needs_free) {
            new_def->title = copy_bytes(old_def->title, old_def->title_size);
            if(new_def->title == NULL && old_def->title_size > 0)
                return -1;
            *retained_bytes += old_def->title_size;
        } else {
            new_def->title = (CHAR*) rebase_source_pointer(old, clone,
                                                           old_def->title);
            if(new_def->title == NULL)
                return -1;
        }
    }
    return 0;
}

static int
clone_ctx(const MD_CTX* old, const char* new_source, MD_SIZE new_size,
          MD_CTX* clone, size_t* retained_bytes)
{
    ptrdiff_t current_offset = -1;
    size_t bytes;
    *clone = *old;
    clone->text = new_source;
    clone->size = new_size;
    clone->doc_ends_with_newline =
        new_size > 0 && ISNEWLINE_(new_source[new_size - 1]);
    clone->max_ref_def_output =
        16 * MIN(new_size, (MD_SIZE) (1024 * 1024 / 16));
    *retained_bytes = 0;

    if(old->current_block != NULL)
        current_offset = (char*) old->current_block - (char*) old->block_bytes;
    bytes = old->alloc_block_bytes;
    clone->block_bytes = copy_bytes(old->block_bytes, bytes);
    if(bytes > 0 && clone->block_bytes == NULL)
        return -1;
    *retained_bytes += bytes;
    clone->current_block = current_offset < 0
                         ? NULL
                         : (MD_BLOCK*) ((char*) clone->block_bytes + current_offset);

    bytes = old->alloc_containers * sizeof(MD_CONTAINER);
    clone->containers = copy_bytes(old->containers, bytes);
    if(bytes > 0 && clone->containers == NULL)
        return -1;
    *retained_bytes += bytes;

    bytes = old->alloc_buffer * sizeof(CHAR);
    clone->buffer = copy_bytes(old->buffer, bytes);
    if(bytes > 0 && clone->buffer == NULL)
        return -1;
    *retained_bytes += bytes;

    /* Marks are not used by the block phase, but clone them defensively. */
    bytes = old->alloc_marks * sizeof(MD_MARK);
    clone->marks = copy_bytes(old->marks, bytes);
    if(bytes > 0 && clone->marks == NULL)
        return -1;
    *retained_bytes += bytes;

    if(clone_ref_defs(old, clone, retained_bytes) != 0)
        return -1;
    clone->footnote_hashtable.defs = NULL;
    clone->footnote_hashtable.buckets = NULL;
    clone->footnote_hashtable.n_defs = 0;
    clone->footnote_hashtable.alloc_defs = 0;
    clone->footnote_hashtable.n_buckets = 0;
    return 0;
}

static void
free_ctx(MD_CTX* ctx)
{
    md_free_ref_defs(ctx);
    md_free_footnote_defs(ctx);
    free(ctx->buffer);
    free(ctx->marks);
    free(ctx->block_bytes);
    free(ctx->containers);
}

static int
same_ref_defs(const MD_CTX* left, const MD_CTX* right)
{
    unsigned i;
    if(left->ref_def_hashtable.n_defs != right->ref_def_hashtable.n_defs)
        return FALSE;
    for(i = 0; i < left->ref_def_hashtable.n_defs; i++) {
        const MD_REF_DEF* a = &left->ref_def_hashtable.ref_defs[i];
        const MD_REF_DEF* b = &right->ref_def_hashtable.ref_defs[i];
        if(a->entry.label_size != b->entry.label_size ||
           memcmp(a->entry.label, b->entry.label, a->entry.label_size) != 0 ||
           a->title_size != b->title_size ||
           memcmp(a->title, b->title, a->title_size) != 0 ||
           a->dest_beg != b->dest_beg || a->dest_end != b->dest_end)
            return FALSE;
    }
    return TRUE;
}

static int
block_data_is_semantic(const MD_BLOCK* block)
{
    return (block->flags & MD_BLOCK_CONTAINER) != 0 ||
           block->type == MD_BLOCK_H || block->type == MD_BLOCK_CODE ||
           block->type == MD_BLOCK_TABLE;
}

static int
tapes_equal(const MD_CTX* left, const MD_CTX* right)
{
    int left_off = 0;
    int right_off = 0;
    while(left_off < left->n_block_bytes && right_off < right->n_block_bytes) {
        const MD_BLOCK* a = (const MD_BLOCK*) ((const char*) left->block_bytes + left_off);
        const MD_BLOCK* b = (const MD_BLOCK*) ((const char*) right->block_bytes + right_off);
        int container = (a->flags & MD_BLOCK_CONTAINER) != 0;
        MD_SIZE i;
        if(a->type != b->type || a->flags != b->flags ||
           a->n_lines != b->n_lines ||
           (block_data_is_semantic(a) && a->data != b->data))
            return FALSE;
        if(container != ((b->flags & MD_BLOCK_CONTAINER) != 0))
            return FALSE;
        left_off += sizeof(MD_BLOCK);
        right_off += sizeof(MD_BLOCK);
        if(container)
            continue;
        if(a->type == MD_BLOCK_CODE || a->type == MD_BLOCK_HTML) {
            const MD_VERBATIMLINE* a_lines = (const MD_VERBATIMLINE*) (a + 1);
            const MD_VERBATIMLINE* b_lines = (const MD_VERBATIMLINE*) (b + 1);
            for(i = 0; i < a->n_lines; i++) {
                if(a_lines[i].beg != b_lines[i].beg ||
                   a_lines[i].end != b_lines[i].end ||
                   a_lines[i].indent != b_lines[i].indent)
                    return FALSE;
            }
            left_off += a->n_lines * sizeof(MD_VERBATIMLINE);
            right_off += b->n_lines * sizeof(MD_VERBATIMLINE);
        } else {
            const MD_LINE* a_lines = (const MD_LINE*) (a + 1);
            const MD_LINE* b_lines = (const MD_LINE*) (b + 1);
            for(i = 0; i < a->n_lines; i++) {
                if(a_lines[i].beg != b_lines[i].beg ||
                   a_lines[i].end != b_lines[i].end)
                    return FALSE;
            }
            left_off += a->n_lines * sizeof(MD_LINE);
            right_off += b->n_lines * sizeof(MD_LINE);
        }
    }
    return left_off == left->n_block_bytes && right_off == right->n_block_bytes;
}

static int
run_case(const char* name, const char* old_source, const char* new_source,
         OFF checkpoint)
{
    MD_CTX prefix;
    MD_CTX resumed;
    MD_CTX clean;
    PARSE_CURSOR prefix_cursor;
    PARSE_CURSOR resumed_cursor;
    PARSE_CURSOR clean_cursor;
    size_t retained = 0;
    int same;
    int tape_exact;
    int same_refs;
    int ret;

    if(strncmp(old_source, new_source, checkpoint) != 0) {
        fprintf(stderr, "%s: edited source does not preserve prefix\n", name);
        return 1;
    }
    init_ctx(&prefix, old_source, (MD_SIZE) strlen(old_source));
    init_cursor(&prefix_cursor);
    ret = parse_to(&prefix, &prefix_cursor, checkpoint);
    if(ret != 0)
        return 1;
    resumed_cursor = prefix_cursor;
    ret = clone_ctx(&prefix, new_source, (MD_SIZE) strlen(new_source),
                    &resumed, &retained);
    if(ret != 0)
        return 1;
    ret = finish_parse(&resumed, &resumed_cursor);
    if(ret != 0)
        return 1;

    init_ctx(&clean, new_source, (MD_SIZE) strlen(new_source));
    init_cursor(&clean_cursor);
    ret = finish_parse(&clean, &clean_cursor);
    if(ret != 0)
        return 1;
    tape_exact = tapes_equal(&resumed, &clean);
    same_refs = same_ref_defs(&resumed, &clean);
    same = tape_exact && same_refs;
    printf("case=%s checkpoint=%u prefix_tape=%d retained_clone_bytes=%zu "
           "final_tape=%d resumed_tape=%d refs=%u tape_exact=%d "
           "refs_exact=%d exact=%d\n",
           name, checkpoint, prefix.n_block_bytes, retained,
           clean.n_block_bytes, resumed.n_block_bytes,
           clean.ref_def_hashtable.n_defs, tape_exact, same_refs, same);

    free_ctx(&prefix);
    free_ctx(&resumed);
    free_ctx(&clean);
    return same ? 0 : 1;
}

int
main(void)
{
    int failures = 0;
    const char* list_prefix = "- alpha\n  continuation\n";
    const char* html_prefix = "<!-- open\n";
    const char* table_prefix = "| a | b |\n";
    const char* ref_prefix = "[same]: /first \"one\"\n\n";

    failures += run_case(
        "list",
        "- alpha\n  continuation\n- old\n",
        "- alpha\n  continuation\n\n  loose continuation\n- new\n",
        (OFF) strlen(list_prefix));
    failures += run_case(
        "html",
        "<!-- open\nold -->\n\nafter\n",
        "<!-- open\nnew\nstill -->\n\nafter\n",
        (OFF) strlen(html_prefix));
    failures += run_case(
        "table",
        "| a | b |\n|---|---|\n| old | row |\n",
        "| a | b |\n|:---|---:|\n| new | row |\n",
        (OFF) strlen(table_prefix));
    failures += run_case(
        "reference",
        "[same]: /first \"one\"\n\n[same]\nold\n",
        "[same]: /first \"one\"\n\n[same]\nnew **value**\n",
        (OFF) strlen(ref_prefix));
    printf("summary cases=4 failures=%d\n", failures);
    return failures == 0 ? 0 : 1;
}
