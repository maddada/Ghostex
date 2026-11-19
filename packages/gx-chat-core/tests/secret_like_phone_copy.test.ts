/**
 * The phone's copy of the key rule (`apps/mobile/app/src/chat/rust/secretLike.ts`) against the strings the core's rule
 * is checked against (`fixtures/secret_like.json`, read by `secret_like.rs`'s unit test), so the two copies agree.
 *
 * The phone keeps no tests of its own (#557, the user: "remove all phone tests"), so its copy is checked here. It is
 * loaded with Node's type stripping rather than through Vite, whose TypeScript transform looks up the phone's
 * `tsconfig.json` (`extends: expo/tsconfig.base`) and cannot resolve it from the repository root.
 */
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

const here = path.dirname(fileURLToPath(import.meta.url));
type Part = string | [alphabet: string, length: number];
const fixture = JSON.parse(readFileSync(path.join(here, 'fixtures/secret_like.json'), 'utf8')) as {
  flagged: string[];
  ordinary: string[];
  not_a_fragment: string[];
  generated: { seed: number; count: number; alphabets: Record<string, string>; formats: Part[][] };
};
/** A `{}` in a fixture string splits a token, so no whole one is written in the file. */
const joined = (text: string) => text.replaceAll('{}', '');

/** The keys `generated` describes, built as `secret_like.rs`'s `generated_keys` builds them. */
function generatedKeys(): string[] {
  const { seed, count, alphabets, formats } = fixture.generated;
  let state = seed >>> 0;
  const next = () => {
    state = (state ^ (state << 13)) >>> 0;
    state = (state ^ (state >>> 17)) >>> 0;
    state = (state ^ (state << 5)) >>> 0;
    return state;
  };
  const keys: string[] = [];
  for (const format of formats) {
    for (let i = 0; i < count; i++) {
      let key = '';
      for (const part of format) {
        if (typeof part === 'string') {
          key += joined(part);
          continue;
        }
        const alphabet = alphabets[part[0]];
        for (let n = 0; n < part[1]; n++) key += alphabet[next() % alphabet.length];
      }
      keys.push(key);
    }
  }
  return keys;
}

test('the phone copy flags what the core flags', async () => {
  const source = readFileSync(path.join(here, '../../../apps/mobile/app/src/chat/rust/secretLike.ts'), 'utf8');
  const javascript = stripTypeScriptTypes(source);
  const { looksLikeSecret } = (await import(
    `data:text/javascript;base64,${Buffer.from(javascript).toString('base64')}`
  )) as { looksLikeSecret: (text: string) => boolean };
  for (const text of fixture.flagged.map(joined)) expect(looksLikeSecret(text), JSON.stringify(text)).toBe(true);
  for (const text of [...fixture.ordinary, ...fixture.not_a_fragment].map(joined)) {
    expect(looksLikeSecret(text), JSON.stringify(text)).toBe(false);
  }
  const keys = generatedKeys();
  expect(keys.length).toBe(fixture.generated.formats.length * fixture.generated.count);
  for (const key of keys) expect(looksLikeSecret(key), `missed a generated key: ${key}`).toBe(true);
});
