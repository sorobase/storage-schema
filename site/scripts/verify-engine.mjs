// SPDX-License-Identifier: Apache-2.0
// pnpm test (SSC-027): the browser engine, loaded through the page's own glue
// (src/lib/engine.ts), must give the native CLI's output byte for byte:
//   1. every scenario's decode report equals fixtures/golden/<id>.decode.json;
//   2. inspect of every demo wasm equals fixtures/golden/<wasm>.inspect.json;
//   3. lint of token + OZ sidecar equals fixtures/golden/lint-token-oz.json;
//   4. live-mode helpers build the keys found in the recorded RPC captures;
//   5. bad input is an error, not a crash.
// Offline: reads only committed files.
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { loadEngine, EngineError } from '../src/lib/engine.ts';

const SITE = join(dirname(fileURLToPath(import.meta.url)), '..');
const ROOT = join(SITE, '..');
const text = (p) => readFileSync(join(ROOT, p), 'utf8');
const golden = (f) => JSON.parse(text(`fixtures/golden/${f}`));
const engine = await loadEngine(readFileSync(join(SITE, 'public/engine/storage-schema-engine.wasm')));
const scenarios = JSON.parse(readFileSync(join(SITE, 'src/data/scenarios.json'), 'utf8'));

let checks = 0;
let failures = 0;
const check = (cond, msg) => {
  checks++;
  if (!cond) {
    failures++;
    console.error(`FAIL ${msg}`);
  }
};

for (const s of scenarios) {
  const inputs = {
    wasm: s.wasm ? new Uint8Array(readFileSync(join(ROOT, s.wasm.file))) : undefined,
    sidecars: s.sidecars.map((c) => ({ label: c.label, text: text(c.file) })),
  };
  const report = engine.decode(inputs, s.entries.map((e) => ({ label: e.label, text: text(e.file) })));
  check(isDeepStrictEqual(report, golden(`${s.id}.decode.json`)), `decode ${s.id} differs from golden`);
}

for (const f of readdirSync(join(ROOT, 'fixtures/golden')).filter((f) => f.endsWith('.inspect.json'))) {
  const wasm = new Uint8Array(readFileSync(join(ROOT, 'fixtures/wasm', f.replace('.inspect.json', '.wasm'))));
  check(isDeepStrictEqual(engine.inspect({ wasm }), golden(f)), `inspect ${f} differs from golden`);
}

const lint = engine.lint({
  wasm: new Uint8Array(readFileSync(join(ROOT, 'fixtures/wasm/ssc_demo_token.wasm'))),
  sidecars: [{ label: 'oz-fungible-pausable-v0.8.0.storage.json', text: text('sidecars/oz-fungible-pausable-v0.8.0.storage.json') }],
});
check(isDeepStrictEqual(lint, golden('lint-token-oz.json')), 'lint token+oz differs from golden');

const usdc = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
check(text('fixtures/mainnet/usdc-sac.instance.json').includes(engine.instanceKey(usdc)), 'instance key not in the recorded response');
const sac = [{ label: 'stellar-asset-contract.storage.json', text: text('sidecars/stellar-asset-contract.storage.json') }];
const holder = 'CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE';
check(text('fixtures/mainnet/usdc-sac.balances.json').includes(engine.entryKey({ sidecars: sac }, usdc, 'DataKey::Balance', [holder])), 'balance key not in the recorded response');
const ex = engine.executable(text('fixtures/mainnet/holder.instance.json'));
check(ex.executable === 'wasm' && ex.wasm_hash === 'd691135aade93ff0f7c229e009cde042130a05124cf7202b03d11246b4f9b473', 'holder executable');
check(engine.codeKey(ex.wasm_hash).length > 40, 'code key');

const throws = (fn) => {
  try {
    fn();
    return false;
  } catch (e) {
    return e instanceof EngineError;
  }
};
check(throws(() => engine.inspect({ wasm: new Uint8Array([1, 2, 3]) })), 'garbage wasm must be an EngineError');
check(throws(() => engine.decode({}, [{ label: 'x', text: '{' }])), 'bad JSON must be an EngineError');
check(throws(() => engine.instanceKey('GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN')), 'account id is not a contract');
check(throws(() => engine.decode({ code: { response: text('fixtures/mainnet/usdc-sac.instance.json'), hash: '00'.repeat(32) } }, [])), 'no code entry must be an error');

if (failures) {
  console.error(`verify-engine: ${failures} of ${checks} checks failed`);
  process.exit(1);
}
console.log(`verify-engine: ${checks} checks pass (${scenarios.length} scenarios equal their CLI goldens; inspect, lint and live-key helpers agree)`);
