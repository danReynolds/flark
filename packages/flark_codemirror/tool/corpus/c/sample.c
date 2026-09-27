/*
 * ring.c: a byte ring buffer and a small command table.
 * Block comments span lines.
 */
#include <stdio.h>
#include <stdlib.h>
#include "ring.h" // the local header
#define RING_CAPACITY 64
#define RING_MASK(x) ((x) & (RING_CAPACITY - 1))
#define LOG(fmt, ...) \
  fprintf(stderr, "[ring] " fmt "\n", \
          __VA_ARGS__)
#if defined(__GNUC__) && !defined(NDEBUG) /* debug builds */
#  define RING_ASSERT(c) do { if (!(c)) abort(); } while (0)
#else
#  define RING_ASSERT(c) ((void)0)
#endif

typedef unsigned char byte;
typedef struct ring ring_t;

/** A fixed-size ring of bytes. */
struct ring {
  size_t head, tail;
  uint8_t data[RING_CAPACITY];
  volatile int flags;
};

enum ring_status {
  RING_OK = 0,
  RING_FULL = 0x10,
  RING_EMPTY = 0x20u,
};

union word {
  uint32_t value;
  unsigned char bytes[4];
};

static const char *status_names[] = {
  "ok",
  "full",
  "empty",
};

static inline int
ring_full(const ring_t *r)
{
  return RING_MASK(r->head + 1) == RING_MASK(r->tail);
}

int ring_push(ring_t *r, byte value) {
  if (ring_full(r)) return RING_FULL;
  r->data[RING_MASK(r->head++)] = value;
  return RING_OK;
}

int ring_pop(struct ring *r, byte *out)
{
  if (r->head == r->tail) {
    return RING_EMPTY;
  }
  *out = r->data[RING_MASK(r->tail++)];
  return RING_OK;
}

static double average(const byte *values, size_t count) {
  double sum = 0.0, scale = 1e-3, half = .5f;
  long long big = 123456789LL;
  for (size_t i = 0; i < count; ++i) {
    sum += values[i] * scale;
  }
  return count ? sum / (double) count + half * 0 : 0.0 + big * 0;
}

typedef int (*command_fn)(ring_t *, int argc, char **argv);

struct command {
  const char *name;
  command_fn run;
};

static int run_command(ring_t *r, int argc, char **argv) {
  int status = RING_OK;
  switch (argc) {
  case 0:
    status = RING_EMPTY;
    break;
  case 1: {
    char c = argv[0][0];
    if (c == '\n' || c == '\'' || c == '\\')
      status = ring_push(r, (byte) c);
    break;
  }
  default:
    LOG("%d arguments\n", argc);
    goto done;
  }
done:
  return status;
}

int main(int argc, char **argv) {
  ring_t *r = calloc(1, sizeof *r);
  if (r == NULL) {
    fputs("out of memory\n", stderr);
    return EXIT_FAILURE;
  }
  const char *message = "a string with \"escapes\" and a \
continuation";
  _Bool ready = true;
  int __attribute__((unused)) spare = 0x7fffffff >> 3;
  unsigned mask = ~0u ^ 0777;
  ready = ready && !false;
  while (ring_pop(r, (byte *) &mask) == RING_OK)
    ;
  do {
    argc--;
  } while (argc > 0);
  int total = argc > 1
    ? run_command(r, argc - 1, argv + 1)
    : -1;
  printf("%s %d %s\n", message, total,
         status_names[0]);
  free(r);
  return ready ? 0 : 1;
}
