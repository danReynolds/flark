"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
["{" "${"] @open.brace
"}" @close.brace
[(string) (template_string) (regex) (regex_pattern)] @opaque
(template_substitution) @code
[(comment) (html_comment)] @comment
(ERROR ["\"" "'" "`"] @opaque.tail)
