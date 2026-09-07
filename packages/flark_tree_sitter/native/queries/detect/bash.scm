["if" "while" "for"] @signal.weak
["then" "fi" "do" "done" "case" "esac"] @signal.strong
((command_name (word) @signal) (#match? @signal "^(echo|printf|export|source|sudo|cd|ls|grep|curl|mkdir)$"))
