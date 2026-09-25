/**
 * Compares the Rust copy of the client-storage catalog (packages/client-storage-native/src/storage_catalog.rs),
 * which the desktop's start-up migrations read, with `definitions` in packages/client-storage/catalog.ts.
 * Order, id, key, collection flag, backend and version must all match. Exits 1 on any difference.
 *
 * Usage: bun tooling/client-storage/catalog-parity.ts
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { definitions } from '@/packages/client-storage/catalog';

const root = fileURLToPath(new URL('../../', import.meta.url));
const source = readFileSync(`${root}packages/client-storage-native/src/storage_catalog.rs`, 'utf8');
const backends: Record<string, string> = { Local: 'local', Session: 'session', Records: 'indexeddb' };
const rust = [
  ...source.matchAll(/store\(\s*"([^"]+)",\s*"([^"]*)",\s*(true|false),\s*(Local|Session|Records),?\s*\)/g),
].map(([, id, key, collection, backend]) => `${id}\t${key}\t${collection}\t${backends[backend]}\t1\tno-upgrade`);
const typescript = definitions.map(
  (definition) =>
    `${definition.id}\t${definition.key}\t${definition.collection}\t${definition.backend}\t${definition.version}\t${
      definition.upgrade ? 'upgrade' : 'no-upgrade'
    }`
);
let differences = 0;
for (let index = 0; index < Math.max(rust.length, typescript.length); index++) {
  if (rust[index] === typescript[index]) continue;
  differences++;
  console.log(`#${index}\n  ts:   ${typescript[index] ?? '(none)'}\n  rust: ${rust[index] ?? '(none)'}`);
}
console.log(
  `${typescript.length} TypeScript definitions, ${rust.length} Rust definitions, ${differences} differences.`
);
process.exit(differences ? 1 : 0);
