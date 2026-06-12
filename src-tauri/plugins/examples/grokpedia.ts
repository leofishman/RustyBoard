/**
 * RustyBoard Example Plugin: Grokpedia Search (TypeScript)
 * =========================================================
 *
 * This is an example of a RustyBoard plugin written in TypeScript.
 * It demonstrates how you can use different runtimes (like Deno, Bun, or Node.js via tsx)
 * to process clipboard text.
 *
 * This plugin reads a term from the clipboard (via stdin), simulates a search
 * on "Grokpedia", and returns a Markdown-formatted result.
 *
 * To run this plugin with Bun, for example, your JSON config should be:
 * {
 *   "command": "bun",
 *   "args": ["run", "grokpedia.ts"]
 * }
 */

async function main() {
    // 1. Read input from stdin
    const decoder = new TextDecoder();
    let input = "";

    for await (const chunk of process.stdin) {
        input += decoder.decode(chunk);
    }

    const query = input.trim();

    if (!query) {
        console.log("Error: Please copy a search term before running the Grokpedia plugin.");
        process.exit(0);
    }

    // 2. Perform the logic (e.g., querying an API)
    // In a real plugin, you would use fetch() to hit the Grok API or Wikipedia.
    // For this example, we return a simulated generated response.

    const simulatedResponse = `
### 🤖 Grokpedia Result

**Query:** \`${query}\`

Grokpedia states that **${query}** is a fascinating subject with many interesting aspects.
It is widely discussed in tech and science circles.

*Note: This is a simulated response. In a real scenario, this would be fetched from an AI model or encyclopedia API!*

**References:**
- [Grok Search](https://grok.com/search?q=${encodeURIComponent(query)})
`;

    // 3. Return the result to stdout
    // RustyBoard will capture this Markdown and display it in a beautiful card.
    console.log(simulatedResponse.trim());
}

main().catch(err => {
    console.error("Plugin crashed:", err);
});
