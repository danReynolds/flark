"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
(string) @opaque
(interpolation) @code
(comment) @comment
(ERROR (string_start) @opaque.tail)
[
 (if_statement ":" @indent.end)
 (elif_clause ":" @indent.end)
 (else_clause ":" @indent.end)
 (for_statement ":" @indent.end)
 (while_statement ":" @indent.end)
 (try_statement ":" @indent.end)
 (except_clause ":" @indent.end)
 (finally_clause ":" @indent.end)
 (with_statement ":" @indent.end)
 (function_definition ":" @indent.end)
 (class_definition ":" @indent.end)
 (match_statement ":" @indent.end)
 (case_clause ":" @indent.end)
]
; A try header is incomplete until an except/finally clause is authored.
(ERROR "try" . ":" @indent.end)
"if" @anchor.if
["for" "while"] @anchor.loop
"try" @anchor.try
"elif" @branch.if
"else" @branch.if.loop.try
["except" "finally"] @branch.try
