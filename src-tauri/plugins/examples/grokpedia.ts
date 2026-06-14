/**
 * RustyBoard Example Plugin: Grokipedia Search (TypeScript)
 * =========================================================
 *
 * Performs a REAL search against Grokipedia (xAI's open encyclopedia,
 * https://grokipedia.com) using its public typeahead endpoint, and returns
 * the top results as Markdown so RustyBoard can render them in a card.
 *
 * It reads the search term from the clipboard (via stdin), queries the API,
 * and prints Markdown to stdout.
 *
 * Recommended runtime: Bun (has a global `fetch`).
 *   {
 *     "command": "bun",
 *     "args": ["run", "grokpedia.ts"]
 *   }
 *
 * It also works with Node.js >= 18 (global fetch) via `tsx`:
 *   { "command": "npx", "args": ["tsx", "grokpedia.ts"] }
 */

const ENDPOINT = "https://grokipedia.com/api/typeahead";
const PAGE_BASE = "https://grokipedia.com/page";
const MAX_RESULTS = 5;

interface TypeaheadResult {
    slug: string;
    title: string;
    snippet?: string;
}

interface TypeaheadResponse {
    results?: TypeaheadResult[];
    error?: string;
}

async function readStdin(): Promise<string> {
    const decoder = new TextDecoder();
    let input = "";
    for await (const chunk of process.stdin) {
        input += decoder.decode(chunk);
    }
    return input.trim();
}

function clean(text: string): string {
    return text
        // Convert MediaWiki bold/italic markup to Markdown ('''x''' -> **x**, ''x'' -> *x*).
        .replace(/'''(.+?)'''/g, "**$1**")
        .replace(/''(.+?)''/g, "*$1*")
        // Collapse whitespace so a multi-line snippet stays on one Markdown line.
        .replace(/\s+/g, " ")
        .trim();
}

async function main() {
    const query = await readStdin();

    if (!query) {
        console.log("Error: Please copy a search term before running the Grokipedia plugin.");
        process.exit(0);
    }

    const url = `${ENDPOINT}?query=${encodeURIComponent(query)}`;

    let res: Response;
    try {
        res = await fetch(url, {
            headers: { "Accept": "application/json" },
            signal: AbortSignal.timeout(15000),
        });
    } catch (err) {
        console.error(`Failed to reach Grokipedia: ${(err as Error).message}`);
        process.exit(1);
    }

    if (!res.ok) {
        console.error(`Grokipedia returned HTTP ${res.status} ${res.statusText}`);
        process.exit(1);
    }

    const data = (await res.json()) as TypeaheadResponse;

    if (data.error) {
        console.error(`Grokipedia error: ${data.error}`);
        process.exit(1);
    }

    const results = (data.results ?? []).slice(0, MAX_RESULTS);

    if (results.length === 0) {
        console.log(`### 🤖 Grokipedia\n\nNo results found for **${query}**.`);
        process.exit(0);
    }

    const lines: string[] = [];
    lines.push(`### 🤖 Grokipedia results for "${query}"`);
    lines.push("");

    for (const r of results) {
        const title = clean(r.title || r.slug);
        const link = `${PAGE_BASE}/${encodeURIComponent(r.slug)}`;
        lines.push(`**[${title}](${link})**`);
        if (r.snippet) {
            lines.push("");
            lines.push(clean(r.snippet));
        }
        lines.push("");
    }

    console.log(lines.join("\n").trim());
}

main().catch((err) => {
    console.error("Plugin crashed:", err);
    process.exit(1);
});
