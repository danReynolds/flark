// C build portability only; parser and scanner sources remain upstream.
// XML expects wchar_t through wctype.h; Clang supplies it through stddef.h.
#include <stddef.h>
#include <wctype.h>

// Bash calls isdigit, absent from Tree-sitter's minimal Wasm ctype.h.
// Its upstream iswdigit implements the same ASCII decimal-digit predicate.
#define isdigit(c) iswdigit((wint_t)(c))
