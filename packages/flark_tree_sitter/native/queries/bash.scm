"(" @open.paren
")" @close.paren
"{" @open.brace
"}" @close.brace
[(string) (raw_string) (ansi_c_string) (translated_string)] @opaque
(heredoc_body) @opaque
(command_substitution) @code
(comment) @comment
(if_statement "then" @open.if)
(ERROR "then" @open.if)
"fi" @close.if
"do" @open.loop
"done" @close.loop
"case" @open.case
"esac" @close.case
["else" "elif"] @middle.if
(elif_clause "then" @indent.end)
