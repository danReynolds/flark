// Selected upstream bracket/indent rules. See upstream/manifest.json and README.
const configurationsJson = r'''
{
  "dart": {
    "brackets": [
      [
        "{",
        "}"
      ],
      [
        "[",
        "]"
      ],
      [
        "(",
        ")"
      ]
    ],
    "indentationRules": {},
    "onEnterRules": []
  },
  "javascript": {
    "brackets": [
      [
        "{",
        "}"
      ],
      [
        "[",
        "]"
      ],
      [
        "(",
        ")"
      ]
    ],
    "indentationRules": {
      "decreaseIndentPattern": "^\\s*[\\}\\]\\)].*$",
      "increaseIndentPattern": "^.*(\\{[^}]*|\\([^)]*|\\[[^\\]]*)$",
      "unIndentedLinePattern": "^(\\t|[ ])*[ ]\\*[^/]*\\*/\\s*$|^(\\t|[ ])*[ ]\\*/\\s*$|^(\\t|[ ])*\\*([ ]([^\\*]|\\*(?!/))*)?$",
      "indentNextLinePattern": "^((.*=>\\s*)|((.*[^\\w]+|\\s*)((if|while|for)\\s*\\(.*\\)\\s*|else\\s*)))$"
    },
    "onEnterRules": [
      {
        "beforeText": "^\\s*(\\bcase\\s.+:|\\bdefault:)$",
        "afterText": "^(?!\\s*(\\bcase\\b|\\bdefault\\b))",
        "indent": "indent"
      }
    ]
  },
  "python": {
    "brackets": [
      [
        "{",
        "}"
      ],
      [
        "[",
        "]"
      ],
      [
        "(",
        ")"
      ]
    ],
    "indentationRules": {},
    "onEnterRules": [
      {
        "beforeText": "^\\s*(?:def|class|for|if|elif|else|while|try|with|finally|except|async).*?:\\s*$",
        "indent": "indent"
      }
    ]
  },
  "yaml": {
    "brackets": [
      [
        "{",
        "}"
      ],
      [
        "[",
        "]"
      ],
      [
        "(",
        ")"
      ]
    ],
    "indentationRules": {
      "increaseIndentPattern": "^\\s*.*(:|-) ?(&amp;\\w+)?(\\{[^}\"']*|\\([^)\"']*)?$",
      "decreaseIndentPattern": "^\\s+\\}$"
    },
    "onEnterRules": []
  },
  "typescript": {
    "brackets": [
      [
        "{",
        "}"
      ],
      [
        "[",
        "]"
      ],
      [
        "(",
        ")"
      ]
    ],
    "indentationRules": {
      "decreaseIndentPattern": "^\\s*[\\}\\]\\)].*$",
      "increaseIndentPattern": "^.*(\\{[^}]*|\\([^)]*|\\[[^\\]]*)$",
      "unIndentedLinePattern": "^(\\t|[ ])*[ ]\\*[^/]*\\*/\\s*$|^(\\t|[ ])*[ ]\\*/\\s*$|^(\\t|[ ])*\\*([ ]([^\\*]|\\*(?!/))*)?$",
      "indentNextLinePattern": "^((.*=>\\s*)|((.*[^\\w]+|\\s*)((if|while|for)\\s*\\(.*\\)\\s*|else\\s*)))$"
    },
    "onEnterRules": [
      {
        "beforeText": "^\\s*(\\bcase\\s.+:|\\bdefault:)$",
        "afterText": "^(?!\\s*(\\bcase\\b|\\bdefault\\b))",
        "indent": "indent"
      }
    ]
  }
}
''';
