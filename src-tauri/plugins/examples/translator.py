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
    pip install googletrans==4.0.0-rc1

Note: For production, you'd likely use a stable API, but this demonstrates the concept.
"""

import sys

try:
    from googletrans import Translator
except ImportError:
    # If the dependency is missing, we must print the error to stdout
    # so RustyBoard captures it and the user can see what's wrong.
    print("Error: The 'googletrans' library is not installed.")
    print("Please run: pip install googletrans==4.0.0-rc1")
    sys.exit(0)

def main():
    # 1. Read the input from RustyBoard (via standard input)
    # We read everything until EOF because the clipboard text could be multi-line.
    input_text = sys.stdin.read().strip()

    if not input_text:
        # If the clipboard is empty, just output nothing
        print("Error: No text provided to translate.")
        return

    # 2. Perform the custom logic (in this case, translating)
    translator = Translator()

    try:
        # We translate the text to English (en) by default.
        # A more advanced plugin could parse command line arguments (sys.argv)
        # to allow the user to define the destination language in their .json file.
        result = translator.translate(input_text, dest='en')
        translated_text = result.text

        # 3. Return the result to RustyBoard (via standard output)
        # We use print() which automatically writes to sys.stdout.
        # RustyBoard will take this string and create a new clipboard card!
        print(translated_text)

    except Exception as e:
        # Always output errors safely so RustyBoard doesn't crash,
        # and the user receives feedback.
        print(f"Translation Error: {str(e)}")

if __name__ == "__main__":
    main()
