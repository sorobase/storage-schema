// SPDX-License-Identifier: Apache-2.0
// Loader for the storage-schema browser engine (crates/web compiled to
// wasm32-unknown-unknown). The same file runs in the page and in
// scripts/verify-engine.mjs (Node type stripping), so the test exercises the
// exact glue the page uses. ABI and request shapes: crates/web/src/lib.rs.

export type Status = 'matched' | 'ambiguous' | 'value_mismatch' | 'unknown';
export type Durability = 'instance' | 'persistent' | 'temporary';

export interface Candidate {
  name: string;
  lib?: string;
  schema_source?: string;
  key?: unknown;
  value_conforms: boolean;
  value?: unknown;
  mismatch?: string;
}

export interface DecodedEntry {
  durability: Durability;
  status: Status;
  name?: string;
  lib?: string;
  schema_source?: string;
  key: unknown;
  value: unknown;
  value_type?: string;
  key_shape: string;
  candidates?: Candidate[];
  reason?: string;
  warnings?: string[];
  source: { contract?: string; ledger_entry: string; last_modified_ledger?: number; live_until_ledger?: number };
  raw: { key_xdr: string; val_xdr: string };
}

export interface Report {
  latest_ledger?: number;
  instances?: { contract: string; executable: string; wasm_hash?: string; storage_entries: number; live_until_ledger?: number }[];
  summary: { total: number; by_status: Record<Status, number> };
  entries: DecodedEntry[];
  notes?: string[];
}

export interface InspectKey {
  name: string;
  lib: string;
  durability: Durability;
  key: string;
  value: string;
}

export interface Inspect {
  wasm_bytes: number;
  custom_sections: { name: string | null; bytes: number }[];
  contractstoragev0: { sections: number; payload_bytes: number; keys: InspectKey[]; types: string[] } | null;
  contractspecv0: { entries: number; udts: string[] };
}

export interface LintFinding {
  code: string;
  severity: 'error' | 'warning' | 'info';
  message: string;
  entries?: string[];
}

export interface Named {
  label: string;
  text: string;
}

/** Live code: a getLedgerEntries response with a ContractCode entry, checked against `hash`. */
export interface LiveCode {
  response: string;
  hash: string;
}

export interface Inputs {
  wasm?: Uint8Array;
  code?: LiveCode;
  sidecars?: Named[];
}

export class EngineError extends Error {}

interface Exports {
  memory: WebAssembly.Memory;
  ssc_alloc(len: number): number;
  ssc_free(ptr: number, len: number): void;
  ssc_call(reqPtr: number, reqLen: number, dataPtr: number, dataLen: number): number;
  ssc_out_ptr(): number;
}

export class Engine {
  private readonly x: Exports;
  private readonly enc = new TextEncoder();
  private readonly dec = new TextDecoder();

  constructor(instance: WebAssembly.Instance) {
    this.x = instance.exports as unknown as Exports;
  }

  /** Raw request: JSON in, JSON out. Throws EngineError when `ok` is false. */
  call(request: Record<string, unknown>, data: Uint8Array = new Uint8Array(0)): Record<string, unknown> {
    const req = this.enc.encode(JSON.stringify(request));
    const reqPtr = this.x.ssc_alloc(req.length);
    const dataPtr = this.x.ssc_alloc(data.length);
    let out: Record<string, unknown>;
    try {
      new Uint8Array(this.x.memory.buffer, reqPtr, req.length).set(req);
      new Uint8Array(this.x.memory.buffer, dataPtr, data.length).set(data);
      const n = this.x.ssc_call(reqPtr, req.length, dataPtr, data.length);
      // Read after the call: memory.buffer may have grown and been replaced.
      const bytes = new Uint8Array(this.x.memory.buffer, this.x.ssc_out_ptr(), n).slice();
      out = JSON.parse(this.dec.decode(bytes)) as Record<string, unknown>;
    } finally {
      this.x.ssc_free(reqPtr, req.length);
      this.x.ssc_free(dataPtr, data.length);
    }
    if (out.ok !== true) throw new EngineError(String(out.error ?? 'engine error'));
    return out;
  }

  private withInputs(op: string, i: Inputs, extra: Record<string, unknown> = {}): [Record<string, unknown>, Uint8Array] {
    return [{ op, has_wasm: i.wasm !== undefined, code: i.code ?? null, sidecars: i.sidecars ?? [], ...extra }, i.wasm ?? new Uint8Array(0)];
  }

  version(): { tool: string; version: string; section: string } {
    return this.call({ op: 'version' }) as unknown as { tool: string; version: string; section: string };
  }

  inspect(i: Inputs): Inspect {
    const [req, data] = this.withInputs('inspect', i);
    return this.call(req, data).inspect as Inspect;
  }

  decode(i: Inputs, entries: Named[], latestLedger?: number): Report {
    const [req, data] = this.withInputs('decode', i, { entries, latest_ledger: latestLedger ?? null });
    return this.call(req, data).report as Report;
  }

  lint(i: Inputs): { findings: LintFinding[]; notes: string[] } {
    const [req, data] = this.withInputs('lint', i);
    return this.call(req, data).lint as { findings: LintFinding[]; notes: string[] };
  }

  instanceKey(contract: string): string {
    return String(this.call({ op: 'instance_key', contract }).key);
  }

  codeKey(hash: string): string {
    return String(this.call({ op: 'code_key', hash }).key);
  }

  entryKey(i: Inputs, contract: string, entry: string, args: string[]): string {
    const [req, data] = this.withInputs('entry_key', i, { contract, entry, args });
    return String(this.call(req, data).key);
  }

  executable(response: string): { contract: string; executable: 'wasm' | 'stellar_asset' | 'external_ref'; wasm_hash?: string; latest_ledger?: number } {
    return this.call({ op: 'executable', response }) as unknown as {
      contract: string;
      executable: 'wasm' | 'stellar_asset' | 'external_ref';
      wasm_hash?: string;
      latest_ledger?: number;
    };
  }
}

/** Instantiates the engine from its bytes (the module has no imports). */
export async function loadEngine(bytes: BufferSource): Promise<Engine> {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Engine(instance);
}

/** Lowercase hex SHA-256 via WebCrypto (browser and Node). */
export async function sha256Hex(bytes: BufferSource): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  return Array.from(d, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Fetches the engine and refuses it if its sha256 differs from the provenance file. */
export async function fetchEngine(url: string, expectedSha256: string): Promise<Engine> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  const bytes = await res.arrayBuffer();
  const hex = await sha256Hex(bytes);
  if (hex !== expectedSha256) throw new Error(`engine sha256 ${hex} does not match its provenance ${expectedSha256}`);
  return loadEngine(bytes);
}
