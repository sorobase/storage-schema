// SPDX-License-Identifier: Apache-2.0
// pnpm demo:data (SSC-027). Builds the site's data from committed project
// files only; nothing is invented and nothing is fetched:
//   fixtures/scenarios.json + every input it names -> public/inputs/...
//   fixtures/golden/*                               -> public/golden/...
//   src/data/scenarios.json   scenario list with provenance and golden summaries
//   src/data/size.json        fixtures/size-report.json (measured by build-demos.sh)
//   src/data/engine.json      public/engine/*.provenance.json (build-engine.sh)
//   src/data/inspect-*.json   CLI inspect goldens for the landing and guide pages
//   src/data/lint-token-oz.json
//   src/content/docs/sep-draft.md  drafts/SEP-storage-schema.md, banner added
// Re-run after scripts/build-demos.sh, capture-*.sh, golden.sh or build-engine.sh.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync, statSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SITE = join(dirname(fileURLToPath(import.meta.url)), '..');
const ROOT = join(SITE, '..');
const read = (p) => readFileSync(join(ROOT, p));
const json = (p) => JSON.parse(read(p).toString('utf8'));
const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');
const write = (p, v) => {
  mkdirSync(dirname(join(SITE, p)), { recursive: true });
  writeFileSync(join(SITE, p), typeof v === 'string' ? v : JSON.stringify(v, null, 2) + '\n');
};
const publish = (rel) => {
  const to = join(SITE, 'public/inputs', rel);
  mkdirSync(dirname(to), { recursive: true });
  copyFileSync(join(ROOT, rel), to);
  return `inputs/${rel}`;
};

const scenarios = json('fixtures/scenarios.json');
const mainnet = json('fixtures/mainnet/provenance.json');
const testhost = json('fixtures/testhost/provenance.json');
const wasmProv = json('fixtures/wasm/provenance.json');

const out = scenarios.scenarios.map((s) => {
  const golden = json(`fixtures/golden/${s.id}.decode.json`);
  mkdirSync(join(SITE, 'public/golden'), { recursive: true });
  copyFileSync(join(ROOT, `fixtures/golden/${s.id}.decode.json`), join(SITE, `public/golden/${s.id}.decode.json`));
  const wasm = s.wasm
    ? (() => {
        const b = read(s.wasm);
        const recorded = wasmProv.files[basename(s.wasm)];
        if (!recorded || recorded.sha256 !== sha256(b)) throw new Error(`${s.wasm}: sha256 differs from fixtures/wasm/provenance.json`);
        return { file: s.wasm, url: publish(s.wasm), bytes: b.length, sha256: recorded.sha256 };
      })()
    : null;
  const provenance =
    s.kind === 'recorded'
      ? {
          kind: 'recorded',
          ledger: golden.latest_ledger ?? mainnet.latest_ledger_at_start,
          date: mainnet.captured.slice(0, 10),
          source: mainnet.rpc,
          script: mainnet.script,
          note: 'getLedgerEntries response captured read-only; ledger is the latestLedger the RPC reported',
        }
      : {
          kind: 'fixture',
          date: testhost.captured.slice(0, 10),
          source: 'soroban test host (soroban-sdk 28.0.0)',
          script: testhost.script,
          note: testhost.calls[basename(s.entries[0])] ?? '',
        };
  return {
    id: s.id,
    title: s.title,
    story: s.story,
    deliberate: s.title.startsWith('Deliberate'),
    wasm,
    sidecars: s.sidecars.map((p) => ({ file: p, label: basename(p), url: publish(p) })),
    entries: s.entries.map((p) => ({ file: p, label: p, url: publish(p) })),
    golden_url: `golden/${s.id}.decode.json`,
    summary: golden.summary,
    instances: golden.instances ?? [],
    provenance,
  };
});
write('src/data/scenarios.json', out);

// Sidecars offered in "bring your own".
write(
  'src/data/sidecars.json',
  readdirSync(join(ROOT, 'sidecars'))
    .filter((f) => f.endsWith('.json'))
    .map((f) => ({ label: f, url: publish(`sidecars/${f}`), description: json(`sidecars/${f}`).description ?? '' })),
);

// Demo wasm for "inspect": every stellar-contract-build output with its provenance.
write(
  'src/data/wasm.json',
  Object.entries(wasmProv.files)
    .filter(([f]) => !f.endsWith('.raw.wasm') && !f.endsWith('.noschema.wasm'))
    .map(([f, v]) => ({ file: `fixtures/wasm/${f}`, url: publish(`fixtures/wasm/${f}`), bytes: v.bytes, sha256: v.sha256 })),
);
for (const f of readdirSync(join(ROOT, 'fixtures/golden')).filter((f) => f.endsWith('.inspect.json'))) {
  copyFileSync(join(ROOT, 'fixtures/golden', f), join(SITE, 'public/golden', f));
}
write('src/data/inspect-token.json', json('fixtures/golden/ssc_demo_token.inspect.json'));
write('src/data/inspect-oz-extern.json', json('fixtures/golden/ssc_demo_oz_extern.inspect.json'));
write('src/data/lint-token-oz.json', json('fixtures/golden/lint-token-oz.json'));
copyFileSync(join(ROOT, 'fixtures/golden/lint-token-oz.json'), join(SITE, 'public/golden/lint-token-oz.json'));

write('src/data/provenance.json', {
  mainnet: { captured: mainnet.captured, rpc: mainnet.rpc, ledger_at_start: mainnet.latest_ledger_at_start, usdc_sac: mainnet.usdc_sac, holder_contract: mainnet.holder_contract, holder_wasm: mainnet.holder_wasm },
  testhost: { captured: testhost.captured, source: testhost.source, rustc: testhost.rustc },
  wasm: { captured: wasmProv.captured, rustc: wasmProv.rustc, target: wasmProv.target, soroban_sdk: wasmProv.soroban_sdk },
});

const size = json('fixtures/size-report.json');
write('src/data/size.json', size);

const engineProv = JSON.parse(readFileSync(join(SITE, 'public/engine/storage-schema-engine.provenance.json'), 'utf8'));
const engineBytes = readFileSync(join(SITE, 'public/engine/storage-schema-engine.wasm'));
if (sha256(engineBytes) !== engineProv.sha256) throw new Error('engine sha256 differs from its provenance file; run scripts/build-engine.sh');
write('src/data/engine.json', engineProv);

// SEP draft page.
const sep = read('drafts/SEP-storage-schema.md').toString('utf8');
write(
  'src/content/docs/sep-draft.md',
  `---\ntitle: SEP draft\ndescription: Contract Storage Schema (informational contractstoragev0 section). Draft, not submitted.\n---\n` +
    `<!-- SPDX-License-Identifier: Apache-2.0 -->\n<!-- Generated by site/scripts/demo-data.mjs from drafts/SEP-storage-schema.md. Edit the draft, not this file. -->\n\n` +
    `:::caution[Draft, not submitted]\nThis is the text in \`drafts/SEP-storage-schema.md\`. It has not been posted to stellar-protocol and has no SEP number. The project owner decides whether and when to submit it.\n:::\n\n` +
    sep,
);

const counts = out.map((s) => `${s.id} ${s.summary.total}`).join(', ');
console.log(`demo:data: ${out.length} scenarios (${counts}); size report ${size.measured}; engine ${engineProv.sha256.slice(0, 12)} (${statSync(join(SITE, 'public/engine/storage-schema-engine.wasm')).size} bytes)`);
