"[" @open.bracket
"]" @close.bracket
"{" @open.brace
"}" @close.brace
[(double_quote_scalar) (single_quote_scalar) (plain_scalar)] @opaque
(comment) @comment
(block_scalar) @opaque.body
(block_scalar ["|" ">"] @indent.scalar)
(block_mapping_pair ":" @indent.end)
(block_sequence_item "-" @indent.prefix)
(ERROR ["\"" "'"] @opaque.tail)
