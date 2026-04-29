import type { Claim, Tier } from './manifest';

export interface ToleranceConstraint {
  metric: string;
  op: '<' | '<=' | '>=' | '>';
  value: number;
}

export interface Profile {
  subsystems?: string[];
  tiers?: Tier[];
  oracles?: string[];
  capabilities?: string[];
  inputClasses?: string[];
  tolerance?: ToleranceConstraint;
  requirePinned?: boolean;
}

export interface MatchOutcome {
  matched: Claim[];
  partial: { claim: Claim; missing: string[] }[];
  rejected: { claim: Claim; missing: string[] }[];
  gaps: string[];
}

const TOLERANCE_DIRECTION: Record<string, 'lower' | 'higher'> = {
  relative_error: 'lower',
  median_relative_error: 'lower',
  absolute_error: 'lower',
  drift: 'lower',
  pass_rate: 'higher',
  recall: 'higher',
  precision: 'higher',
  f1: 'higher',
};

function isPlaceholder(s: string | null | undefined): boolean {
  return typeof s === 'string' && s.startsWith('PENDING-');
}

function isLowerOp(op: string): boolean {
  return op === '<' || op === '<=';
}

function isHigherOp(op: string): boolean {
  return op === '>' || op === '>=';
}

export function toleranceSatisfies(
  claim: Claim,
  ask: ToleranceConstraint
): boolean {
  if (!claim.tolerances) return false;
  const direction = TOLERANCE_DIRECTION[ask.metric];
  if (!direction) return false;
  for (const t of claim.tolerances) {
    if (t.metric !== ask.metric) continue;
    if (t.op == null || t.value == null) continue;
    if (direction === 'lower') {
      if (!isLowerOp(t.op) || !isLowerOp(ask.op)) continue;
      if (t.value <= ask.value) return true;
    } else {
      if (!isHigherOp(t.op) || !isHigherOp(ask.op)) continue;
      if (t.value >= ask.value) return true;
    }
  }
  return false;
}

function evaluate(claim: Claim, profile: Profile): string[] {
  const missing: string[] = [];

  if (profile.tiers?.length && !profile.tiers.includes(claim.tier)) {
    missing.push(`tier (claim is ${claim.tier})`);
  }

  if (profile.subsystems?.length) {
    if (!claim.subsystem || !profile.subsystems.includes(claim.subsystem)) {
      missing.push(
        claim.subsystem
          ? `subsystem (claim is ${claim.subsystem})`
          : 'subsystem (claim has none)'
      );
    }
  }

  if (profile.oracles?.length) {
    const claimOracles = new Set(claim.evidence?.oracle ?? []);
    const hit = profile.oracles.some((o) => claimOracles.has(o));
    if (!hit) {
      missing.push(`oracle (claim has ${[...claimOracles].join(', ') || 'none'})`);
    }
  }

  if (profile.capabilities?.length) {
    const claimCaps = new Set(claim.capabilities ?? []);
    const hit = profile.capabilities.some((c) => claimCaps.has(c));
    if (!hit) missing.push('capability (none of the requested ones)');
  }

  if (profile.inputClasses?.length) {
    const cls = claim.inputs?.class;
    if (!cls || !profile.inputClasses.includes(cls)) {
      missing.push(
        cls ? `input class (claim is ${cls})` : 'input class (claim has none)'
      );
    }
  }

  if (profile.tolerance) {
    if (!toleranceSatisfies(claim, profile.tolerance)) {
      missing.push(
        `tolerance ${profile.tolerance.metric} ${profile.tolerance.op} ${profile.tolerance.value}`
      );
    }
  }

  if (profile.requirePinned) {
    const versions = claim.pinned_versions ?? {};
    const hasPlaceholder = Object.values(versions).some(isPlaceholder);
    const corpusShaPlaceholder = isPlaceholder(claim.inputs?.corpus_sha);
    if (hasPlaceholder || corpusShaPlaceholder) {
      missing.push('pinning (claim has placeholder versions)');
    }
  }

  return missing;
}

function describeGaps(profile: Profile, allClaims: Claim[]): string[] {
  const gaps: string[] = [];
  const measurement = allClaims.filter((c) => c.kind === 'measurement');

  for (const subsystem of profile.subsystems ?? []) {
    const tiers = profile.tiers?.length ? profile.tiers : (['ci', 'release', 'research'] as Tier[]);
    for (const tier of tiers) {
      const hit = measurement.some(
        (c) => c.subsystem === subsystem && c.tier === tier
      );
      if (!hit) {
        gaps.push(
          `No measurement claim for subsystem \`${subsystem}\` at tier \`${tier}\`.`
        );
      }
    }
  }

  for (const cap of profile.capabilities ?? []) {
    const hit = measurement.some((c) => c.capabilities?.includes(cap));
    if (!hit) {
      gaps.push(`No claim advertises capability \`${cap}\`.`);
    }
  }

  if (profile.tolerance) {
    const onMetric = measurement.some((c) =>
      c.tolerances?.some((t) => t.metric === profile.tolerance!.metric)
    );
    if (!onMetric) {
      gaps.push(
        `No claim measures \`${profile.tolerance.metric}\`.`
      );
    }
  }

  return gaps;
}

export function match(allClaims: Claim[], profile: Profile): MatchOutcome {
  const candidates = allClaims.filter((c) => c.kind === 'measurement');

  const matched: Claim[] = [];
  const partial: { claim: Claim; missing: string[] }[] = [];
  const rejected: { claim: Claim; missing: string[] }[] = [];

  for (const c of candidates) {
    const missing = evaluate(c, profile);
    if (missing.length === 0) {
      matched.push(c);
    } else if (missing.length === 1) {
      partial.push({ claim: c, missing });
    } else {
      rejected.push({ claim: c, missing });
    }
  }

  return {
    matched,
    partial,
    rejected,
    gaps: describeGaps(profile, allClaims),
  };
}

export function profileFromQuery(query: URLSearchParams): Profile {
  const splitCsv = (key: string) => {
    const v = query.get(key);
    return v ? v.split(',').filter(Boolean) : undefined;
  };
  const subsystems = splitCsv('subsystem');
  const tiers = splitCsv('tier') as Tier[] | undefined;
  const oracles = splitCsv('oracle');
  const capabilities = splitCsv('capability');
  const inputClasses = splitCsv('class');
  const metric = query.get('metric');
  const op = query.get('op');
  const valueStr = query.get('value');
  const tolerance =
    metric && op && valueStr != null && !Number.isNaN(parseFloat(valueStr))
      ? { metric, op: op as ToleranceConstraint['op'], value: parseFloat(valueStr) }
      : undefined;
  const requirePinned = query.get('pinned') === '1';
  return {
    subsystems,
    tiers,
    oracles,
    capabilities,
    inputClasses,
    tolerance,
    requirePinned: requirePinned || undefined,
  };
}

export function profileToQuery(profile: Profile): string {
  const params = new URLSearchParams();
  if (profile.subsystems?.length) params.set('subsystem', profile.subsystems.join(','));
  if (profile.tiers?.length) params.set('tier', profile.tiers.join(','));
  if (profile.oracles?.length) params.set('oracle', profile.oracles.join(','));
  if (profile.capabilities?.length) params.set('capability', profile.capabilities.join(','));
  if (profile.inputClasses?.length) params.set('class', profile.inputClasses.join(','));
  if (profile.tolerance) {
    params.set('metric', profile.tolerance.metric);
    params.set('op', profile.tolerance.op);
    params.set('value', String(profile.tolerance.value));
  }
  if (profile.requirePinned) params.set('pinned', '1');
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}
