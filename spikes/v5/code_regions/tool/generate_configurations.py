"""Extract the explicitly supported subset from the pinned upstream inputs.

This is not a general JSONC parser or a VS Code extension/config interpreter.
The normalization handles the inspected files only; hashes pin those inputs.
"""

from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parents[1]
manifest = {
    entry["language"]: entry
    for entry in json.loads((root / "upstream/manifest.json").read_text())
}
configurations = {}
for language in ["dart", "javascript", "python", "yaml"]:
    raw = (root / "upstream" / (language + ".jsonc")).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == manifest[language]["sha256"]
    text = re.sub(r"(?m)^\s*//.*\n", "", raw.decode())
    text = re.sub(r",\s*([}\]])", r"\1", text)
    data = json.loads(text)

    def pattern(value):
        return value.get("pattern") if isinstance(value, dict) else value

    configurations[language] = {
        "brackets": [
            pair for pair in data["brackets"] if len(pair[0]) == len(pair[1]) == 1
        ],
        "indentationRules": {
            key: pattern(value)
            for key, value in data.get("indentationRules", {}).items()
        },
        "onEnterRules": [
            {
                **{key: pattern(value) for key, value in rule.items() if key != "action"},
                "indent": rule["action"]["indent"],
            }
            for rule in data.get("onEnterRules", [])
            if rule["action"] == {"indent": "indent"}
        ],
    }

# Deliberately shares the tested JS editing subset, with a separate TS grammar.
configurations["typescript"] = configurations["javascript"]
(root / "lib/configurations.dart").write_text(
    "// Selected upstream bracket/indent rules. See upstream/manifest.json and README.\n"
    "const configurationsJson = r'''\n"
    + json.dumps(configurations, indent=2)
    + "\n''';\n"
)
