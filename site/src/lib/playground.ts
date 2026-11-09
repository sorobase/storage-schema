// SPDX-License-Identifier: Apache-2.0
// Playground logic (SSC-027). Everything is computed in this page by the
// storage-schema engine (src/lib/engine.ts). Sources and their labels:
//   scenario  inputs served from this site; chip = the capture's provenance
//             (recorded mainnet capture with ledger and date, or test-host
//             fixture); the result is compared with the CLI golden file.
//   own       files the visitor supplied; chip = "Your files".
//   live      getLedgerEntries calls made by this page to a public mainnet
//             RPC; chip = "Live read" with the ledger the RPC reported.
// Nothing here writes to any network.
import { fetchEngine, type DecodedEntry, type Engine, type Inputs, type Inspect, type LintFinding, type Named, type Report, type Status } from './engine.ts';

interface Scenario {
  id: string;
  title: string;
  story: string;
  deliberate: boolean;
  wasm: { file: string; url: string; bytes: number; sha256: string } | null;
  sidecars: { file: string; label: string; url: string }[];
  entries: { file: string; label: string; url: string }[];
  golden_url: string;
  provenance: { kind: 'recorded' | 'fixture'; ledger?: number; date: string; source: string; script: string; note: string };
}

interface Boot {
  base: string;
  engine: { sha256: string; size: number; rustc: string; built_at: string };
  scenarios: Scenario[];
  sidecars: { label: string; url: string; description: string }[];
  wasm: { file: string; url: string; bytes: number; sha256: string }[];
  rpc: string;
}

type Source =
  | { kind: 'recorded'; ledger: number; date: string; rpc: string; note: string }
  | { kind: 'fixture'; date: string; source: string; note: string }
  | { kind: 'own'; names: string[] }
  | { kind: 'live'; ledger: number; at: string; rpc: string; sidecars: string[] };

const STATUS_ORDER: Status[] = ['matched', 'ambiguous', 'value_mismatch', 'unknown'];
const STATUS_WORD: Record<Status, string> = { matched: 'Matched', ambiguous: 'Ambiguous', value_mismatch: 'Value mismatch', unknown: 'Unknown' };
const STATUS_HELP: Record<Status, string> = {
  matched: 'One schema entry claims this key and the value has the declared type.',
  ambiguous: 'More than one schema entry claims this key. The decoder lists them and does not pick.',
  value_mismatch: 'One schema entry claims the key, but the stored value does not have its type.',
  unknown: 'No schema entry claims this key. Nothing is named or guessed.',
};

// ---- small DOM helpers -------------------------------------------------------

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function icon(name: string): Node {
  const t = document.getElementById(`ss-icon-${name}`) as HTMLTemplateElement | null;
  return t ? t.content.cloneNode(true) : document.createTextNode('');
}

const fmt = (n: number) => n.toLocaleString('en-US');
const utcNow = () => {
  const iso = new Date().toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 16)} UTC`;
};

/** Strkeys and hashes are middle-truncated; the full value stays in the title. */
function short(s: string): string {
  return /^[A-Z0-9]{56}$/.test(s) || /^[0-9a-f]{64}$/.test(s) ? `${s.slice(0, 6)}…${s.slice(-6)}` : s;
}

function shortDeep(v: unknown): unknown {
  if (typeof v === 'string') return short(v);
  if (Array.isArray(v)) return v.map(shortDeep);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v as Record<string, unknown>).map(([k, x]) => [k, shortDeep(x)]));
  return v;
}

function compact(v: unknown): string {
  if (typeof v === 'string') return short(v);
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(shortDeep(v));
}

/** `Balance(GA5Z…KZVN)` for a tagged key; the stellar-xdr JSON otherwise. */
function keyText(k: unknown): string {
  if (k && typeof k === 'object' && 'tag' in (k as Record<string, unknown>)) {
    const t = k as { tag: string; fields?: unknown[] };
    return t.fields && t.fields.length ? `${t.tag}(${t.fields.map(compact).join(', ')})` : t.tag;
  }
  return compact(k);
}

/** A `Enum::Variant` name that may wrap after `::`, never inside a word. */
function breakable(text: string, cls: string): HTMLElement {
  const span = el('span', cls);
  text.split('::').forEach((part, i) => {
    if (i > 0) span.append('::', document.createElement('wbr'));
    span.append(part);
  });
  return span;
}

function statusLabel(s: Status, count?: number): HTMLElement {
  const span = el('span', `ss-status ss-st-${s}`);
  span.append(icon(s), el('span', undefined, count === undefined ? STATUS_WORD[s] : `${fmt(count)} ${STATUS_WORD[s].toLowerCase()}`));
  span.title = STATUS_HELP[s];
  return span;
}

/** Provenance chip (design/DESIGN.md 8.2). */
function chip(src: Source): HTMLElement {
  const c = el('span', 'ss-chip');
  const parts: string[] = [];
  let title = '';
  if (src.kind === 'live') {
    c.append(el('span', 'ss-live-dot'));
    parts.push('Live read', `ledger ${fmt(src.ledger)}`, src.at, 'mainnet', src.sidecars.length ? `sidecar: ${src.sidecars.join(', ')}` : 'no sidecar');
    title = `Read by this page from ${src.rpc} with getLedgerEntries (read-only). Ledger is the latestLedger the RPC reported. ${src.sidecars.length ? `Schema applied from: ${src.sidecars.join(', ')}.` : 'No sidecar applied: only a schema inside the contract wasm can match.'}`;
  } else if (src.kind === 'recorded') {
    c.append(icon('recorded'));
    parts.push('Recorded mainnet capture', `ledger ${fmt(src.ledger)}`, src.date);
    title = `Captured read-only from ${src.rpc} by scripts/capture-mainnet.sh; ${src.note}. Served from this site.`;
  } else if (src.kind === 'fixture') {
    c.append(icon('fixture'));
    parts.push('Fixture', src.date, 'soroban test host');
    title = `${src.source}, scripts/capture-testhost.sh. Calls: ${src.note}. DEMO-ONLY contracts, not a network.`;
  } else {
    c.append(icon('own'));
    parts.push('Your files', ...src.names.map(short));
    title = 'Files you supplied, decoded in this browser.';
  }
  const ps = el('span', 'ss-chip-parts');
  parts.forEach((p, i) => ps.append(el('span', i === 0 ? 'ss-chip-kind' : 'ss-chip-atomic', p)));
  c.append(ps);
  c.title = title;
  c.setAttribute('aria-label', `Provenance: ${parts.join(', ')}. ${title}`);
  return c;
}

function canonical(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(canonical);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.keys(v as Record<string, unknown>)
        .sort()
        .map((k) => [k, canonical((v as Record<string, unknown>)[k])]),
    );
  }
  return v;
}
const sameJson = (a: unknown, b: unknown) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));

async function getText(url: string): Promise<string> {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`HTTP ${r.status} for ${url}`);
  return r.text();
}
async function getBytes(url: string): Promise<Uint8Array> {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`HTTP ${r.status} for ${url}`);
  return new Uint8Array(await r.arrayBuffer());
}

// ---- result rendering ----------------------------------------------------------

interface View {
  title: string;
  story?: string;
  deliberate?: boolean;
  source: Source;
  report: Report;
  inspect?: Inspect | null;
  inspectNote?: string;
  lint?: { findings: LintFinding[] } | null;
  golden?: { url: string; equal: boolean };
  calls?: string[];
}

function entryRow(e: DecodedEntry): HTMLElement {
  const d = el('details', `ss-entry ss-entry-${e.status}`);
  const s = el('summary', 'ss-entry-sum');
  const name = el('span', 'ss-entry-name');
  if (e.name) {
    name.append(breakable(e.name, 'ss-mono ss-strong'));
    if (e.lib) name.append(el('span', 'ss-lib', e.lib));
  } else {
    name.append(el('span', 'ss-unknown-name', e.status === 'ambiguous' ? `${e.candidates?.length ?? 0} candidates` : 'Unnamed'));
  }
  // Ambiguous entries keep the untyped key; every candidate renders it the same way, so show the first.
  const shownKey = e.status === 'unknown' ? e.key_shape : keyText(e.status === 'ambiguous' && e.candidates?.[0]?.key ? e.candidates[0].key : e.key);
  const key = el('span', 'ss-mono ss-entry-key', shownKey);
  key.title = JSON.stringify(e.key);
  const val = el('span', 'ss-mono ss-entry-val', compact(e.value));
  val.title = JSON.stringify(e.value);
  s.append(statusLabel(e.status), el('span', 'ss-dur', e.durability), name, key, val);
  d.append(s);

  const body = el('div', 'ss-entry-body');
  const dl = el('dl', 'ss-dl');
  const row = (k: string, v: string | Node) => {
    dl.append(el('dt', undefined, k));
    const dd = el('dd');
    dd.append(v);
    dl.append(dd);
  };
  row('Status', `${STATUS_WORD[e.status]}. ${STATUS_HELP[e.status]}`);
  if (e.reason) row('Reason', e.reason);
  if (e.value_type) row('Declared value type', el('code', undefined, e.value_type));
  row('Key shape', el('code', undefined, e.key_shape));
  if (e.schema_source) row('Schema from', e.schema_source);
  const src = e.source;
  row(
    'Ledger entry',
    `${src.ledger_entry === 'instance_storage' ? 'element of the instance storage map' : 'contract data'}${src.contract ? ` of ${src.contract}` : ''}` +
      `${src.last_modified_ledger !== undefined ? `, last modified at ledger ${fmt(src.last_modified_ledger)}` : ''}` +
      `${src.live_until_ledger !== undefined ? `, live until ledger ${fmt(src.live_until_ledger)}` : ''}`,
  );
  for (const w of e.warnings ?? []) row('Warning', w);
  body.append(dl);
  if (e.candidates?.length) {
    const t = el('table', 'ss-cands');
    const head = el('tr');
    ['Candidate', 'Writer (lib)', 'Value fits', 'Rendered value'].forEach((h) => head.append(el('th', undefined, h)));
    t.append(el('thead'), el('tbody'));
    t.tHead!.append(head);
    for (const c of e.candidates) {
      const tr = el('tr');
      tr.append(el('td', 'ss-mono', c.name), el('td', 'ss-mono', c.lib || 'the contract'), el('td', undefined, c.value_conforms ? 'Yes' : 'No'), el('td', 'ss-mono', compact(c.value ?? '')));
      t.tBodies[0]!.append(tr);
    }
    body.append(el('p', 'ss-sub-title', 'Candidates (the decoder does not choose)'), t);
  }
  body.append(el('p', 'ss-sub-title', 'Decoded value (full)'));
  body.append(el('pre', 'ss-pre', JSON.stringify(e.value, null, 2)));
  body.append(el('p', 'ss-sub-title', 'Raw XDR, as stored'));
  body.append(el('pre', 'ss-pre ss-raw', `key  ${e.raw.key_xdr}\nval  ${e.raw.val_xdr}`));
  d.append(body);
  return d;
}

function schemaPanel(i: Inspect | null | undefined, note?: string): HTMLElement | null {
  if (i === undefined) return null;
  const sec = el('section', 'ss-subpanel');
  sec.append(el('h3', 'ss-h3', 'What the wasm carries'));
  if (!i) {
    sec.append(el('p', 'ss-note', note ?? 'No wasm in this input, so no embedded schema. Only sidecars apply.'));
    return sec;
  }
  const s = i.contractstoragev0;
  if (!s) {
    sec.append(el('p', 'ss-note', `${fmt(i.wasm_bytes)}-byte wasm with no contractstoragev0 section. Custom sections: ${i.custom_sections.map((c) => c.name ?? '?').join(', ')}.`));
    return sec;
  }
  sec.append(
    el(
      'p',
      'ss-note',
      `${fmt(i.wasm_bytes)}-byte wasm. contractstoragev0: ${s.keys.length} key entries, ${s.types.length} type entries, ${fmt(s.payload_bytes)} payload bytes. ` +
        `contractspecv0 keeps ${i.contractspecv0.udts.length} user type(s)${i.contractspecv0.udts.length ? ` (${i.contractspecv0.udts.join(', ')})` : ''}.`,
    ),
  );
  const t = el('table', 'ss-table');
  const head = el('tr');
  ['Name', 'Durability', 'Key pattern', 'Value type', 'Written by'].forEach((h) => head.append(el('th', undefined, h)));
  t.append(el('thead'), el('tbody'));
  t.tHead!.append(head);
  for (const k of s.keys) {
    const tr = el('tr');
    tr.append(el('td', 'ss-mono', k.name), el('td', undefined, k.durability), el('td', 'ss-mono', k.key), el('td', 'ss-mono', k.value), el('td', 'ss-mono', k.lib || 'the contract'));
    t.tBodies[0]!.append(tr);
  }
  const wrap = el('div', 'ss-table-wrap');
  wrap.append(t);
  sec.append(wrap);
  if (s.types.length) sec.append(el('p', 'ss-note', `Type definitions carried with the schema: ${s.types.join(', ')}.`));
  return sec;
}

function lintPanel(l: { findings: LintFinding[] } | null | undefined): HTMLElement | null {
  if (!l) return null;
  const relevant = l.findings.filter((f) => f.severity !== 'info');
  const sec = el('section', 'ss-subpanel');
  sec.append(el('h3', 'ss-h3', 'Lint, before deploy'));
  if (!relevant.length) {
    sec.append(el('p', 'ss-note', `No collisions or undefined types (${l.findings.length} informational finding(s)).`));
    return sec;
  }
  const ul = el('ul', 'ss-lint');
  for (const f of relevant) {
    const li = el('li');
    const sev = el('span', `ss-status ${f.severity === 'error' ? 'ss-st-value_mismatch' : 'ss-st-ambiguous'}`);
    sev.append(icon(f.severity === 'error' ? 'value_mismatch' : 'ambiguous'), el('span', undefined, `${f.code}`));
    li.append(sev, el('span', undefined, f.message));
    ul.append(li);
  }
  sec.append(ul);
  return sec;
}

function render(v: View): void {
  const body = document.getElementById('ss-result-body')!;
  document.getElementById('ss-result-empty')!.hidden = true;
  body.hidden = false;
  body.replaceChildren();

  const head = el('div', 'ss-result-head');
  head.append(el('h2', 'ss-result-title', v.title));
  const meta = el('div', 'ss-row');
  meta.append(chip(v.source));
  if (v.deliberate) meta.append(el('span', 'ss-tag', 'Deliberate combination of real inputs'));
  head.append(meta);
  if (v.story) head.append(el('p', 'ss-story', v.story));
  body.append(head);

  const r = v.report;
  const sum = el('div', 'ss-summary');
  sum.setAttribute('aria-live', 'polite');
  sum.append(el('span', 'ss-total', `${fmt(r.summary.total)} ${r.summary.total === 1 ? 'entry' : 'entries'}`));
  for (const s of STATUS_ORDER) if (r.summary.by_status[s] > 0) sum.append(statusLabel(s, r.summary.by_status[s]));
  body.append(sum);

  if (v.golden) {
    const g = el('p', `ss-golden ${v.golden.equal ? 'ss-golden-ok' : 'ss-golden-bad'}`);
    g.append(icon(v.golden.equal ? 'check' : 'x'));
    const a = el('a', undefined, v.golden.url.split('/').pop());
    a.href = v.golden.url;
    g.append(
      el('span', undefined, v.golden.equal ? 'Identical to the native CLI output in ' : 'Differs from the native CLI output in '),
      a,
      el('span', undefined, v.golden.equal ? '. Computed in this browser.' : '. Please report this.'),
    );
    body.append(g);
  }

  for (const i of r.instances ?? []) {
    body.append(
      el(
        'p',
        'ss-note',
        `Instance of ${i.contract}: executable ${i.executable}${i.wasm_hash ? ` (wasm ${short(i.wasm_hash)})` : ''}, ${fmt(i.storage_entries)} instance storage element(s)` +
          `${i.live_until_ledger !== undefined ? `, live until ledger ${fmt(i.live_until_ledger)}` : ''}.`,
      ),
    );
  }
  for (const n of r.notes ?? []) body.append(el('p', 'ss-note', `Note: ${n}`));

  const list = el('div', 'ss-entries');
  const cols = el('div', 'ss-entry-cols');
  cols.setAttribute('aria-hidden', 'true');
  ['Status', 'Durability', 'Name', 'Key', 'Value'].forEach((c) => cols.append(el('span', undefined, c)));
  list.append(cols);
  if (!r.entries.length) list.append(el('p', 'ss-note', 'No contract-data entries in this input.'));
  r.entries.forEach((e) => list.append(entryRow(e)));
  body.append(list);

  const sp = schemaPanel(v.inspect, v.inspectNote);
  if (sp) body.append(sp);
  const lp = lintPanel(v.lint);
  if (lp) body.append(lp);
  if (v.calls?.length) {
    const sec = el('section', 'ss-subpanel');
    sec.append(el('h3', 'ss-h3', 'Calls this page made'));
    const ol = el('ol', 'ss-calls-list');
    v.calls.forEach((c) => ol.append(el('li', 'ss-mono', c)));
    sec.append(ol);
    body.append(sec);
  }
}

function renderError(title: string, message: string): void {
  const body = document.getElementById('ss-result-body')!;
  document.getElementById('ss-result-empty')!.hidden = true;
  body.hidden = false;
  const box = el('div', 'ss-error');
  box.setAttribute('role', 'alert');
  const h = el('p', 'ss-error-title');
  h.append(icon('value_mismatch'), el('span', undefined, title));
  box.append(h, el('pre', 'ss-pre', message), el('p', 'ss-note', 'Nothing was decoded. The previous result, if any, was cleared.'));
  body.replaceChildren(box);
}

function busy(on: boolean): void {
  document.getElementById('ss-result')?.setAttribute('aria-busy', String(on));
  document.getElementById('ss-result-body')?.classList.toggle('ss-stale', on);
}

// ---- sources -------------------------------------------------------------------

async function runScenario(engine: Engine, boot: Boot, s: Scenario): Promise<void> {
  busy(true);
  try {
    const at = (u: string) => `${boot.base}${u}`;
    const inputs: Inputs = {
      wasm: s.wasm ? await getBytes(at(s.wasm.url)) : undefined,
      sidecars: await Promise.all(s.sidecars.map(async (c) => ({ label: c.label, text: await getText(at(c.url)) }))),
    };
    const entries: Named[] = await Promise.all(s.entries.map(async (e) => ({ label: e.label, text: await getText(at(e.url)) })));
    const report = engine.decode(inputs, entries);
    const golden = JSON.parse(await getText(at(s.golden_url))) as unknown;
    const p = s.provenance;
    const source: Source =
      p.kind === 'recorded'
        ? { kind: 'recorded', ledger: report.latest_ledger ?? p.ledger ?? 0, date: p.date, rpc: p.source, note: p.note }
        : { kind: 'fixture', date: p.date, source: p.source, note: p.note };
    const hasSchema = Boolean(inputs.wasm) || (inputs.sidecars?.length ?? 0) > 0;
    render({
      title: s.title,
      story: s.story,
      deliberate: s.deliberate,
      source,
      report,
      inspect: inputs.wasm ? engine.inspect(inputs) : s.id.startsWith('mainnet-holder') ? undefined : null,
      lint: hasSchema ? engine.lint(inputs) : null,
      golden: { url: at(s.golden_url), equal: sameJson(report, golden) },
    });
  } catch (e) {
    renderError('Could not decode this scenario', String(e instanceof Error ? e.message : e));
  } finally {
    busy(false);
  }
}

async function checkedSidecars(boot: Boot, selector: string): Promise<Named[]> {
  const boxes = Array.from(document.querySelectorAll<HTMLInputElement>(selector)).filter((b) => b.checked);
  return Promise.all(boxes.map(async (b) => ({ label: b.dataset.label ?? 'sidecar.json', text: await getText(`${boot.base}${b.value}`) })));
}

function initOwn(engine: Engine, boot: Boot): void {
  const file = document.getElementById('ss-own-wasm') as HTMLInputElement;
  const demo = document.getElementById('ss-own-wasm-demo') as HTMLSelectElement;
  const note = document.getElementById('ss-own-wasm-note')!;
  let wasm: { bytes: Uint8Array; name: string } | null = null;
  file.addEventListener('change', async () => {
    const f = file.files?.[0];
    if (!f) return;
    demo.value = '';
    wasm = { bytes: new Uint8Array(await f.arrayBuffer()), name: f.name };
    note.textContent = `${f.name}, ${fmt(wasm.bytes.length)} bytes, read locally.`;
  });
  demo.addEventListener('change', async () => {
    if (!demo.value) return;
    file.value = '';
    const name = demo.value.split('/').pop() ?? 'demo.wasm';
    wasm = { bytes: await getBytes(`${boot.base}${demo.value}`), name };
    note.textContent = `${name}, ${fmt(wasm.bytes.length)} bytes, demo contract (DEMO-ONLY).`;
  });
  const gather = async (): Promise<Inputs> => {
    const sidecars = await checkedSidecars(boot, '#ss-pane-own input[type=checkbox]');
    const pasted = (document.getElementById('ss-own-sidecar') as HTMLTextAreaElement).value.trim();
    if (pasted) sidecars.push({ label: 'pasted-sidecar.json', text: pasted });
    return { wasm: wasm?.bytes, sidecars };
  };
  const names = (i: Inputs) => [...(wasm && i.wasm ? [wasm.name] : []), ...(i.sidecars ?? []).map((s) => s.label)];
  document.getElementById('ss-own-run')!.addEventListener('click', async () => {
    busy(true);
    try {
      const i = await gather();
      const text = (document.getElementById('ss-own-entries') as HTMLTextAreaElement).value.trim();
      if (!text) throw new Error('Paste a getLedgerEntries response first.');
      const report = engine.decode(i, [{ label: 'pasted entries', text }]);
      render({
        title: 'Your files',
        source: { kind: 'own', names: [...names(i), 'pasted entries'] },
        report,
        inspect: i.wasm ? engine.inspect(i) : null,
        lint: i.wasm || i.sidecars?.length ? engine.lint(i) : null,
      });
    } catch (e) {
      renderError('Could not decode', String(e instanceof Error ? e.message : e));
    } finally {
      busy(false);
    }
  });
  document.getElementById('ss-own-inspect')!.addEventListener('click', async () => {
    try {
      const i = await gather();
      if (!i.wasm) throw new Error('Choose a wasm file or a demo wasm first.');
      render({
        title: `Inspect ${wasm?.name ?? 'wasm'}`,
        source: { kind: 'own', names: names(i) },
        report: { summary: { total: 0, by_status: { matched: 0, ambiguous: 0, value_mismatch: 0, unknown: 0 } }, entries: [] },
        inspect: engine.inspect(i),
        lint: engine.lint(i),
      });
    } catch (e) {
      renderError('Could not inspect', String(e instanceof Error ? e.message : e));
    }
  });
}

/** Read-only JSON-RPC with at most one call per 600 ms (the programme budget is 2 req/s). */
function makeRpc(url: string, log: (s: string) => void) {
  let last = 0;
  return async (keys: string[], why: string): Promise<{ text: string; latest?: number }> => {
    const wait = last + 600 - Date.now();
    if (wait > 0) await new Promise((r) => setTimeout(r, wait));
    last = Date.now();
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'getLedgerEntries', params: { keys } }),
    });
    if (!res.ok) throw new Error(`RPC HTTP ${res.status}`);
    const text = await res.text();
    const j = JSON.parse(text) as { error?: { message?: string }; result?: { latestLedger?: number; entries?: unknown[] | null } };
    if (j.error) throw new Error(`RPC error: ${j.error.message ?? JSON.stringify(j.error)}`);
    const n = j.result?.entries?.length ?? 0;
    log(`getLedgerEntries (${why}): ${n} entr${n === 1 ? 'y' : 'ies'}, latest ledger ${fmt(j.result?.latestLedger ?? 0)}`);
    return { text, latest: j.result?.latestLedger };
  };
}

interface KeyOpt {
  name: string;
  durability: string;
}

function sidecarKeys(texts: Named[]): KeyOpt[] {
  const out: KeyOpt[] = [];
  for (const t of texts) {
    try {
      const j = JSON.parse(t.text) as { keys?: { name: string; durability: string }[] };
      for (const k of j.keys ?? []) out.push({ name: k.name, durability: k.durability });
    } catch {
      /* the engine reports bad sidecars */
    }
  }
  return out;
}

function initLive(engine: Engine, boot: Boot): void {
  const idIn = document.getElementById('ss-live-id') as HTMLInputElement;
  const fetchSel = document.getElementById('ss-live-fetch') as HTMLSelectElement;
  const argsIn = document.getElementById('ss-live-args') as HTMLInputElement;
  const callsOl = document.getElementById('ss-live-calls')!;
  const run = document.getElementById('ss-live-run') as HTMLButtonElement;
  const sacBox = Array.from(document.querySelectorAll<HTMLInputElement>('input[name=ss-live-sidecar]')).find((b) => b.dataset.label?.startsWith('stellar-asset-contract'));
  let loadedFor = '';
  // The SAC box is ticked for the user when the contract is a Stellar Asset
  // Contract, and cleared again when the next contract is not one. A tick the
  // user made by hand is left alone.
  let sacAuto = false;
  sacBox?.addEventListener('change', () => {
    sacAuto = false;
  });
  let calls: string[] = [];
  const log = (s: string) => {
    calls.push(s);
    callsOl.append(el('li', undefined, s));
  };
  const rpc = makeRpc(boot.rpc, log);
  // Only wasm code is cached (it is addressed by its hash, so it cannot change).
  // Instance and data entries are read again on every click.
  const cache = new Map<string, { text: string; latest?: number }>();
  const cached = async (key: string, why: string) => {
    const hit = cache.get(key);
    if (hit) {
      log(`${why}: reused this session's earlier read (code is immutable for its hash)`);
      return hit;
    }
    const r = await rpc([key], why);
    cache.set(key, r);
    return r;
  };

  idIn.addEventListener('input', () => {
    if (idIn.value.trim() !== loadedFor) {
      fetchSel.disabled = true;
      argsIn.disabled = true;
      fetchSel.replaceChildren(el('option', undefined, 'Read the instance first'));
    }
  });

  run.addEventListener('click', async () => {
    const id = idIn.value.trim();
    calls = [];
    callsOl.replaceChildren();
    run.disabled = true;
    busy(true);
    try {
      const instKey = engine.instanceKey(id);
      const inst = await rpc([instKey], 'contract instance');
      const ex = engine.executable(inst.text);
      if (sacBox) {
        if (ex.executable === 'stellar_asset' && !sacBox.checked) {
          sacBox.checked = true;
          sacAuto = true;
        } else if (ex.executable !== 'stellar_asset' && sacAuto) {
          sacBox.checked = false;
          sacAuto = false;
        }
      }
      let code: Inputs['code'];
      let inspect: Inspect | null = null;
      let inspectNote: string | undefined;
      if (ex.executable === 'wasm' && ex.wasm_hash) {
        const c = await cached(engine.codeKey(ex.wasm_hash), `wasm ${short(ex.wasm_hash)}`);
        code = { response: c.text, hash: ex.wasm_hash };
        inspect = engine.inspect({ code });
      } else {
        inspectNote =
          ex.executable === 'stellar_asset'
            ? 'This is a built-in Stellar Asset Contract: it has no wasm, so it can never carry a section. Use the SAC sidecar.'
            : 'The executable is an external reference; not resolved by this page.';
      }
      const sidecars = await checkedSidecars(boot, 'input[name=ss-live-sidecar]');
      const inputs: Inputs = { code, sidecars };

      // Offer the non-instance keys of whatever schema applies.
      if (loadedFor !== id) {
        const opts: KeyOpt[] = [
          ...(inspect?.contractstoragev0?.keys ?? []).map((k) => ({ name: k.name, durability: k.durability })),
          ...sidecarKeys(sidecars),
        ].filter((k) => k.durability !== 'instance');
        fetchSel.replaceChildren(el('option', undefined, opts.length ? 'None' : 'No non-instance keys in the schema'));
        (fetchSel.options[0] as HTMLOptionElement).value = '';
        for (const o of opts) {
          const op = el('option', undefined, `${o.name} (${o.durability})`);
          op.value = o.name;
          fetchSel.append(op);
        }
        fetchSel.disabled = !opts.length;
        argsIn.disabled = !opts.length;
        loadedFor = id;
      }

      const entries: Named[] = [{ label: 'live instance', text: inst.text }];
      let latest = inst.latest;
      if (fetchSel.value) {
        const args = argsIn.value
          .split(',')
          .map((a) => a.trim())
          .filter(Boolean);
        const key = engine.entryKey(inputs, id, fetchSel.value, args);
        const got = await rpc([key], `${fetchSel.value}(${args.map(short).join(', ')})`);
        entries.push({ label: `live ${fetchSel.value}`, text: got.text });
        latest = got.latest ?? latest;
        const parsed = JSON.parse(got.text) as { result?: { entries?: unknown[] | null } };
        if (!parsed.result?.entries?.length) log(`${fetchSel.value}: no such entry (never written, or archived)`);
      }
      const report = engine.decode(inputs, entries, latest);
      render({
        title: `Contract ${short(id)}`,
        source: { kind: 'live', ledger: latest ?? 0, at: utcNow(), rpc: boot.rpc, sidecars: sidecars.map((x) => x.label) },
        report,
        inspect,
        inspectNote,
        lint: code || sidecars.length ? engine.lint(inputs) : null,
        calls: calls.slice(),
      });
    } catch (e) {
      renderError('Live read failed', String(e instanceof Error ? e.message : e));
    } finally {
      run.disabled = false;
      busy(false);
    }
  });
}

function initTabs(): void {
  const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('.ss-tab'));
  const select = (t: HTMLButtonElement) => {
    for (const x of tabs) {
      const on = x === t;
      x.setAttribute('aria-selected', String(on));
      x.tabIndex = on ? 0 : -1;
      document.getElementById(`ss-pane-${x.dataset.tab}`)!.hidden = !on;
    }
  };
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => select(t));
    t.addEventListener('keydown', (ev) => {
      const d = ev.key === 'ArrowRight' ? 1 : ev.key === 'ArrowLeft' ? -1 : 0;
      if (!d) return;
      const n = tabs[(i + d + tabs.length) % tabs.length]!;
      select(n);
      n.focus();
    });
  });
  const want = new URLSearchParams(location.search).get('tab');
  const t = tabs.find((x) => x.dataset.tab === want);
  if (t) select(t);
}

export async function initPlayground(): Promise<void> {
  const root = document.querySelector<HTMLElement>('.ss-pg');
  if (!root?.dataset.boot) return;
  const boot = JSON.parse(root.dataset.boot) as Boot;
  initTabs();
  const state = document.getElementById('ss-engine-state')!;
  let engine: Engine;
  try {
    engine = await fetchEngine(`${boot.base}engine/storage-schema-engine.wasm`, boot.engine.sha256);
    state.textContent = 'Loaded; sha256 matches its provenance file.';
    const t = document.getElementById('ss-empty-title');
    if (t) t.textContent = 'Pick a scenario, or use your files or a live contract';
  } catch (e) {
    state.textContent = `Not loaded: ${e instanceof Error ? e.message : String(e)}`;
    renderError('The decoder did not load', state.textContent);
    return;
  }
  const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>('.ss-scen'));
  const pick = (id: string, push: boolean) => {
    const s = boot.scenarios.find((x) => x.id === id);
    if (!s) return;
    buttons.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.scenario === id)));
    if (push) history.replaceState(null, '', `?s=${encodeURIComponent(id)}`);
    void runScenario(engine, boot, s);
  };
  buttons.forEach((b) => b.addEventListener('click', () => pick(b.dataset.scenario!, true)));
  initOwn(engine, boot);
  initLive(engine, boot);
  const params = new URLSearchParams(location.search);
  // Default view: the recorded USDC scenario, unless a link names another source.
  if (!params.get('tab')) pick(params.get('s') ?? boot.scenarios[0]!.id, false);
}
