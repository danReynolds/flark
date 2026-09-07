"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
[(string) (subshell) (regex) (delimited_symbol) (string_array) (symbol_array)] @opaque
(heredoc_body) @opaque.body
(interpolation) @code
(comment) @comment
(ERROR ["\"" "`"] @opaque.tail)

; Restrict keyword openers to blocks; postfix conditionals and endless
; methods use different grammar shapes and must not open indentation levels.
(method "def" @open.block "end")
(singleton_method "def" @open.block "end")
[(class "class" @open.block)
 (singleton_class "class" @open.block)
 (module "module" @open.block)
 (if "if" @open.block)
 (unless "unless" @open.block)
 (while "while" @open.block)
 (until "until" @open.block)
 (for "for" @open.block)
 (case "case" @open.block)
 (case_match "case" @open.block)
 (begin "begin" @open.block)
 (do_block "do" @open.block)]
; During ordinary typing the parser may leave a header in an ERROR node
; until its newline/end arrives. These are real keyword tokens, not text.
(ERROR ["def" "class" "module" "if" "unless" "while" "until" "for" "case" "begin" "do"] @open.block)
"end" @close.block
[(else "else" @middle.block)
 (elsif "elsif" @middle.block)
 (when "when" @middle.block)
 (in_clause "in" @middle.block)
 (rescue "rescue" @middle.block)
 (ensure "ensure" @middle.block)]
(ERROR ["else" "elsif" "when" "in" "rescue" "ensure"] @middle.block)
