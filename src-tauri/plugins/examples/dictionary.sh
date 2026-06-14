#!/bin/bash
# RustyBoard Example Plugin: Dictionary Definition
# ================================================
#
# This simple Bash script acts as a plugin for RustyBoard.
# It reads the clipboard content from standard input, assumes it is a single word,
# and fetches its definition from a free dictionary API.

# 1. Read standard input
# 'cat' without arguments reads from stdin. We store it in a variable.
INPUT_WORD=$(cat)

# Strip leading/trailing whitespace
INPUT_WORD=$(echo "$INPUT_WORD" | xargs)

# 2. Validate
if [ -z "$INPUT_WORD" ]; then
    echo "Error: Empty input. Please copy a single word."
else
    # Ensure it's roughly one word by checking spaces (simple check)
    if echo "$INPUT_WORD" | grep -q " "; then
        echo "Error: Expected a single word, but received multiple words."
    else
        # 3. Call the external API
        # We use curl in silent mode (-s) so we only get the JSON response.
        RESPONSE=$(curl -s "https://api.dictionaryapi.dev/api/v2/entries/en/${INPUT_WORD}")

        # 4. Check for errors from the API
        if echo "$RESPONSE" | grep -q '"title":"No Definitions Found"'; then
            echo "No definition found for '$INPUT_WORD'."
        else
            # 5. Extract and format the result as Markdown.
            # We prefer 'jq' (a command-line JSON processor) if available, and
            # otherwise fall back to 'python3' so this works out of the box
            # without installing anything extra.
            if command -v jq &> /dev/null; then
                DEFINITION=$(echo "$RESPONSE" | jq -r '.[0].meanings[0].definitions[0].definition')
                PART_OF_SPEECH=$(echo "$RESPONSE" | jq -r '.[0].meanings[0].partOfSpeech')

                # 6. Output to standard output as Markdown for RustyBoard to render.
                echo "### Dictionary Definition"
                echo "**Word:** \`${INPUT_WORD}\`"
                echo "**Type:** *${PART_OF_SPEECH}*"
                echo "**Definition:** ${DEFINITION}"
            elif command -v python3 &> /dev/null; then
                echo "$RESPONSE" | python3 -c '
import sys, json
word = sys.argv[1]
data = json.load(sys.stdin)
entry = data[0]
meaning = entry["meanings"][0]
definition = meaning["definitions"][0]["definition"]
pos = meaning["partOfSpeech"]
print("### Dictionary Definition")
print(f"**Word:** `{word}`")
print(f"**Type:** *{pos}*")
print(f"**Definition:** {definition}")
' "$INPUT_WORD"
            else
                # Last resort: neither jq nor python3 available.
                echo "Install 'jq' or 'python3' to format definitions. Raw response:"
                echo ""
                echo "$RESPONSE"
            fi
        fi
    fi
fi
