#!/usr/bin/env node
/**
 * Verifies that the committed IDL, the Anchor manifest and the hand-written SDK
 * discriminators still describe the same program.
 *
 * The SDK never generates code from the IDL, so drift is a real failure mode:
 * Anchor derives instruction discriminators from `sha256("global:<name>")` and
 * account discriminators from `sha256("account:<Name>")`. This script recomputes
 * both and fails on any mismatch.
 */
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const idlPath = resolve(root, 'packages/sdk/idl/openwork_escrow.json');
const anchorPath = resolve(root, 'Anchor.toml');
const discriminatorsPath = resolve(root, 'packages/sdk/src/discriminators.ts');

const problems = [];
const check = (condition, message) => {
  if (!condition) problems.push(message);
};

const idl = JSON.parse(readFileSync(idlPath, 'utf8'));
const anchor = readFileSync(anchorPath, 'utf8');
const discriminatorsSource = readFileSync(discriminatorsPath, 'utf8');

const discriminator = (namespace, name) =>
  createHash('sha256').update(`${namespace}:${name}`).digest().subarray(0, 8);

const hex = (bytes) => Buffer.from(bytes).toString('hex');

// ---------------------------------------------------------------- program id --
const manifestIds = [...anchor.matchAll(/openwork_escrow\s*=\s*"([^"]+)"/g)].map(
  (match) => match[1],
);
check(manifestIds.length > 0, 'Anchor.toml declares no openwork_escrow program id');
for (const id of manifestIds) {
  check(id === idl.address, `Anchor.toml program id ${id} != IDL address ${idl.address}`);
}
check(
  /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(idl.address),
  `IDL address is not a base58 public key: ${idl.address}`,
);

// -------------------------------------------------------------- instructions --
const idlInstructions = new Map(
  idl.instructions.map((instruction) => [instruction.name, instruction]),
);
check(idlInstructions.size === 9, `expected 9 instructions, found ${idlInstructions.size}`);

for (const [name, instruction] of idlInstructions) {
  const expected = discriminator('global', name);
  const actual = Uint8Array.from(instruction.discriminator ?? []);
  check(
    hex(actual) === hex(expected),
    `IDL discriminator for ${name} is ${hex(actual)}, expected ${hex(expected)}`,
  );
  check(instruction.accounts?.length > 0, `${name} declares no accounts`);
}

// -------------------------------------------------------------- SDK constants --
const instructionBlock = discriminatorsSource.match(
  /IX_DISCRIMINATORS\s*=\s*\{([\s\S]*?)\}\s*as const/,
);
check(instructionBlock !== null, 'IX_DISCRIMINATORS not found in discriminators.ts');
const sdkInstructions = new Map(
  [...(instructionBlock?.[1] ?? '').matchAll(/(\w+):\s*'([0-9a-f]{16})'/g)].map(
    (match) => [match[1], match[2]],
  ),
);

check(
  sdkInstructions.size === idlInstructions.size,
  `SDK has ${sdkInstructions.size} instructions, IDL has ${idlInstructions.size}`,
);
for (const [name, instruction] of idlInstructions) {
  const expected = hex(Uint8Array.from(instruction.discriminator));
  check(
    sdkInstructions.get(name) === expected,
    `SDK discriminator for ${name} is ${sdkInstructions.get(name)}, expected ${expected}`,
  );
}

// ------------------------------------------------------------------- accounts --
for (const account of idl.accounts ?? []) {
  const expected = hex(discriminator('account', account.name));
  const actual = hex(Uint8Array.from(account.discriminator ?? []));
  check(
    actual === expected,
    `IDL account discriminator for ${account.name} is ${actual}, expected ${expected}`,
  );
  const sdkValue = new RegExp(`${account.name}:\\s*'([0-9a-f]{16})'`).exec(
    discriminatorsSource,
  )?.[1];
  check(
    sdkValue === expected,
    `SDK account discriminator for ${account.name} is ${sdkValue}, expected ${expected}`,
  );
}

// --------------------------------------------------------------------- events --
for (const event of idl.events ?? []) {
  const expected = hex(discriminator('event', event.name));
  const actual = hex(Uint8Array.from(event.discriminator ?? []));
  check(
    actual === expected,
    `IDL event discriminator for ${event.name} is ${actual}, expected ${expected}`,
  );
}

if (problems.length > 0) {
  console.error('IDL check failed:');
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}

console.log(
  `IDL ok: ${idlInstructions.size} instructions, ${idl.accounts.length} accounts, ` +
    `${idl.events.length} events, program ${idl.address}`,
);
