; Flark snippet editing captures; see CONTRACT.md.
["(" ] @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
(string_literal) @opaque
(template_substitution) @code
[(comment) (block_comment) (documentation_block_comment)] @comment
; An unfinished literal may be recovered as ERROR rather than a string node.
(ERROR ["\"" "'" "\"\"\"" "'''"] @opaque.tail)
(ERROR "/" . "*" @opaque.tail)
