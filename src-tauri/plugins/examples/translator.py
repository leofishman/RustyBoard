#!/usr/bin/env python3
"""
RustyBoard Example Plugin: Simple Translator
============================================

This script serves as a community template for creating RustyBoard plugins.
RustyBoard interacts with plugins entirely via Standard Input/Output.

How it works:
1. RustyBoard reads the configuration file (translator.json).
2. When the user clicks the plugin button in the UI, RustyBoard executes
   the command specified in the JSON file.
3. RustyBoard takes the *raw clipboard content* and pipes it directly into
   this script's standard input (sys.stdin).
4. The script processes the text.
5. The script prints the result to standard output (sys.stdout).
6. RustyBoard captures the stdout, sanitizes it (to prevent XSS and leaks),
   and stores it as a *new* clipboard entry.

Dependencies:
    None! This uses only the Python standard library (urllib) and Google's
    public `gtx` translation endpoint, so it works out of the box with any
    Python 3 install and auto-detects the source language.

You can change the target language by editing TARGET_LANG below (or by passing
it as an argument in translator.json, e.g. "args": ["translator.py", "es"]).
"""

import sys
import json
import urllib.parse
import urllib.request

# Default target language. Overridable via the first CLI argument (sys.argv[1]).
TARGET_LANG = "en"
ENDPOINT = "https://translate.googleapis.com/translate_a/single"


def translate(text: str, target: str) -> str:
    params = urllib.parse.urlencode({
        "client": "gtx",
        "sl": "auto",      # auto-detect source language
        "tl": target,      # target language
        "dt": "t",         # request translated text
        "q": text,
    })
    url = f"{ENDPOINT}?{params}"

    req = urllib.request.Request(url, headers={"User-Agent": "RustyBoard-Plugin/1.0"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    # The response is a nested array; translated segments live at data[0][i][0].
    segments = data[0] or []
    return "".join(seg[0] for seg in segments if seg and seg[0])


def main():
    # 1. Read the input from RustyBoard (via standard input).
    # We read everything until EOF because the clipboard text could be multi-line.
    input_text = sys.stdin.read().strip()

    if not input_text:
        print("Error: No text provided to translate.")
        return

    target = sys.argv[1] if len(sys.argv) > 1 else TARGET_LANG

    try:
        # 2. Perform the translation and print it to stdout, where RustyBoard
        # captures it and creates a new clipboard card.
        print(translate(input_text, target))
    except Exception as e:
        # Always output errors safely so RustyBoard doesn't crash,
        # and the user receives feedback.
        print(f"Translation Error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
