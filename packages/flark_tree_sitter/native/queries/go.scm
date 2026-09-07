"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
[(interpreted_string_literal) (raw_string_literal) (rune_literal)] @opaque
(comment) @comment
