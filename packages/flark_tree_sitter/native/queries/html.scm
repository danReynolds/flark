; Void elements do not open an indentation level. Names are upstream tokens.
((start_tag (tag_name) @_name) @open.tag
 (#not-match? @_name "(?i)^(area|base|br|col|embed|hr|img|input|link|meta|param|source|track|wbr)$"))
(end_tag) @close.tag
(quoted_attribute_value) @opaque
(comment) @comment
(raw_text) @opaque
