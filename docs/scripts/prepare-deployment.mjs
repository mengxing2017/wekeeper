import { createHash } from "node:crypto";
import { readdir, readFile, mkdir, writeFile, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const docs = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(docs, ".vitepress/dist");
async function files(root) {
  const entries = await readdir(root, { withFileTypes: true });
  return (
    await Promise.all(
      entries.map((e) =>
        e.isDirectory() ? files(join(root, e.name)) : join(root, e.name),
      ),
    )
  ).flat();
}
const hashes = new Set();
const htmlFiles = (await files(dist)).filter((f) => f.endsWith(".html"));
if (!htmlFiles.length) throw new Error("No built documentation found");
for (const file of htmlFiles) {
  const html = await readFile(file, "utf8");
  if (!html.includes("/docs/assets/"))
    throw new Error(`Missing /docs/ asset base: ${file}`);
  for (const [, attrs, body] of html.matchAll(
    /<script\b([^>]*)>([\s\S]*?)<\/script>/gi,
  )) {
    if (!/\bsrc\s*=/i.test(attrs) && body.trim()) {
      hashes.add(
        `'sha256-${createHash("sha256").update(body).digest("base64")}'`,
      );
    }
  }
  // Catch missing local resources; VitePress already rejects broken Markdown links.
  for (const [, url] of html.matchAll(
    /(?:src|href)="(\/docs\/[^"?#]+)(?:[?#][^"]*)?"/g,
  )) {
    if (!/\.(?:js|css|png|jpg|jpeg|webp|svg|woff2?|ico)$/i.test(url)) continue;
    await stat(join(dist, decodeURIComponent(url.slice("/docs/".length))));
  }
}
const policy = `default-src 'self'; script-src 'self' ${[...hashes].sort().join(" ")}; connect-src 'self'; img-src 'self' data: https:; font-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`;
await mkdir(join(docs, ".vitepress/caddy"), { recursive: true });
await writeFile(
  join(docs, ".vitepress/caddy/docs.caddy"),
  `# Generated from this build's inline scripts; do not edit.\n(docs_csp) {\n  header Content-Security-Policy "${policy}"\n}\n`,
);
console.log(
  `Validated ${htmlFiles.length} documents and generated CSP with ${hashes.size} inline script hashes.`,
);
