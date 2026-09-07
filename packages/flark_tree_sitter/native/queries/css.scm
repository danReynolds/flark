"(" @open.paren
")" @close.paren
"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
(string_value) @opaque
(comment) @comment
; Incomplete quotes and the upstream double-quoted-brace recovery can expose
; literal punctuation as ERROR children. Preserve whitespace until resolved.
(ERROR ["\"" "'"] @opaque.tail)
