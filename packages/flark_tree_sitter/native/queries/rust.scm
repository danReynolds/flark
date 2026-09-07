"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
[(string_literal) (raw_string_literal) (char_literal)] @opaque
[(line_comment) (block_comment)] @comment
