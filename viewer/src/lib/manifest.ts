import * as fs from 'node:fs';
import * as path from 'node:path';
import YAML from 'yaml';

export type Kind = 'measurement' | 'policy' | 'reference';
export type Tier = 'ci' | 'release' | 'research';
export type Provenance = 'automatic' | 'human' | 'peer-reviewed';

export interface Reviewer {
  name: string;
  orcid?: string;
  affiliation?: string;
  date?: string;
}

export interface Tolerance {
  metric?: string;
  op?: string;
  value?: number;
  prose: string;
}

export interface Inputs {
  corpus?: string;
  n?: number;
  class?: string;
  corpus_sha?: string;
  fixture_path?: string;
}

export interface OutputDef {
  unit?: string;
  description?: string;
}

export interface LastVerified {
  commit?: string | null;
  date?: string | null;
  value?: number | null;
  corpus_sha?: string | null;
}

export interface Evidence {
  oracle: string[];
  command: string;
  artifact: string;
}

export interface Claim {
  id: string;
  title: string;
  kind: Kind;
  subsystem?: string;
  case: string;
  source: string;
  tier: Tier;
  trust_strategy: string[];
  pattern?: string;
  capabilities?: string[];
  inputs?: Inputs;
  outputs?: Record<string, OutputDef>;
  pinned_versions?: Record<string, string>;
  claim: string;
  tolerances?: Tolerance[];
  evidence: Evidence;
  provenance: Provenance;
  reviewers?: Reviewer[];
  last_verified?: LastVerified;
  assumptions: string[];
  failure_modes: string[];
  // injected by loader
  project: string;
  manifestPath: string;
}

export interface Vocabularies {
  subsystem: string[];
  oracle: string[];
  capability: string[];
  tolerance_metric: string[];
  tolerance_op: string[];
  input_class: string[];
}

export interface Manifest {
  project: string;
  version: string;
  vocabularies: Vocabularies;
  claims: Claim[];
  manifestPath: string;
}

const BASE_VOCAB: Vocabularies = {
  tolerance_metric: [
    'relative_error',
    'median_relative_error',
    'absolute_error',
    'pass_rate',
    'recall',
    'precision',
    'f1',
    'drift',
  ],
  tolerance_op: ['<', '<=', '>=', '>', '=='],
  input_class: ['single-chain', 'multi-chain', 'random-sample', 'synthetic', 'fixture'],
  subsystem: [],
  oracle: [],
  capability: [],
};

function readYaml(filePath: string): any {
  return YAML.parse(fs.readFileSync(filePath, 'utf-8'));
}

function normalizeClaim(raw: any, project: string, manifestPath: string): Claim {
  return {
    kind: raw.kind ?? 'measurement',
    provenance: (raw.provenance ?? 'automatic') as Provenance,
    project,
    manifestPath,
    ...raw,
  };
}

function loadManifestFile(filePath: string): Manifest {
  const root = path.dirname(filePath);
  const data = readYaml(filePath);
  if (!data || typeof data !== 'object') {
    throw new Error(`Manifest is not a mapping: ${filePath}`);
  }
  const project = String(data.project ?? 'unknown');
  const claims: Claim[] = [];
  for (const c of data.claims ?? []) {
    claims.push(normalizeClaim(c, project, filePath));
  }
  for (const include of data.include ?? []) {
    const incPath = path.resolve(root, include);
    const inc = readYaml(incPath);
    for (const c of inc.claims ?? []) {
      claims.push(normalizeClaim(c, project, filePath));
    }
  }
  const declared = (data.vocabularies ?? {}) as Partial<Record<keyof Vocabularies, string[]>>;
  const vocabularies: Vocabularies = {
    tolerance_metric: [...new Set([...BASE_VOCAB.tolerance_metric, ...(declared.tolerance_metric ?? [])])],
    tolerance_op: [...new Set([...BASE_VOCAB.tolerance_op, ...(declared.tolerance_op ?? [])])],
    input_class: [...new Set([...BASE_VOCAB.input_class, ...(declared.input_class ?? [])])],
    subsystem: [...new Set([...BASE_VOCAB.subsystem, ...(declared.subsystem ?? [])])],
    oracle: [...new Set([...BASE_VOCAB.oracle, ...(declared.oracle ?? [])])],
    capability: [...new Set([...BASE_VOCAB.capability, ...(declared.capability ?? [])])],
  };
  return {
    project,
    version: String(data.version ?? '0'),
    vocabularies,
    claims,
    manifestPath: filePath,
  };
}

// Manifest paths come from EVIDENT_MANIFESTS (colon-separated, like $PATH).
// Each entry resolves against the viewer's working directory, so relative
// paths point out of viewer/ into the surrounding repo. Default to the
// framework's own example manifest so a fresh clone builds without setup.
const DEFAULT_MANIFESTS = ['../evident.yaml'];
const MANIFEST_PATHS = (process.env.EVIDENT_MANIFESTS ?? '')
  .split(':')
  .map((p) => p.trim())
  .filter(Boolean);
const RESOLVED_PATHS = (MANIFEST_PATHS.length ? MANIFEST_PATHS : DEFAULT_MANIFESTS).map(
  (p) => path.resolve(p),
);

export const manifests: Manifest[] = RESOLVED_PATHS.filter(fs.existsSync).map(loadManifestFile);
export const allClaims: Claim[] = manifests.flatMap((m) => m.claims);

export function uniqueSorted(values: Iterable<string>): string[] {
  return [...new Set(values)].filter(Boolean).sort();
}
