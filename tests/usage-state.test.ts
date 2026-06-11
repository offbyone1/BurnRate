import { strict as assert } from 'node:assert';
import test from 'node:test';
import type { ClaudeUsageResponse, CodexUsage } from '../src/types.ts';
import { resolveMode } from '../src/source-toggle.ts';
import {
  keepLastGoodOnClaudeFailure,
  keepLastGoodOnCodexFailure,
  formatOptionalUtilization,
  describeClaudeFailure,
  describeCodexFailure,
  usageForRender,
} from '../src/usage-state.ts';

const usage: ClaudeUsageResponse = {
  five_hour: { utilization: 42, resets_at: null },
  seven_day: { utilization: 7, resets_at: null },
  extra_usage: null,
};

const codex: CodexUsage = {
  planType: 'pro',
  primary: { utilization: 21, windowMinutes: 300, resetsAt: null },
  secondary: { utilization: 4, windowMinutes: 10080, resetsAt: null },
  snapshotAt: '2026-06-09T00:00:00.000Z',
};

test('source toggle determines compact layout even while data is temporarily unavailable', () => {
  assert.equal(resolveMode({ claude: true, codex: true }, false, true), 'both');
  assert.equal(resolveMode({ claude: true, codex: true }, true, false), 'both');
  assert.equal(resolveMode({ claude: true, codex: false }, false, false), 'claude');
  assert.equal(resolveMode({ claude: false, codex: true }, false, false), 'codex');
  assert.equal(resolveMode({ claude: false, codex: false }, true, true), 'none');
});

test('claude polling failures keep the last good claude usage payload', () => {
  assert.equal(keepLastGoodOnClaudeFailure(usage), usage);
  assert.equal(keepLastGoodOnClaudeFailure(null), null);
});

test('source toggles can re-render before claude has a last successful payload', () => {
  assert.equal(usageForRender(usage), usage);
  assert.deepEqual(usageForRender(null), {
    five_hour: null,
    seven_day: null,
    extra_usage: null,
  });
});

test('codex polling failures keep the last good codex snapshot', () => {
  assert.equal(keepLastGoodOnCodexFailure(codex), codex);
  assert.equal(keepLastGoodOnCodexFailure(null), null);
});

test('missing live utilization renders as neutral placeholder, not 0 percent or error text', () => {
  assert.equal(formatOptionalUtilization(undefined), '--');
  assert.equal(formatOptionalUtilization(null), '--');
  assert.equal(formatOptionalUtilization(0), '0%');
  assert.equal(formatOptionalUtilization(14.4), '14%');
});

test('claude failures stay out of user-facing chrome', () => {
  const missingAuth = describeClaudeFailure(
    'Could not read C:\\Users\\me\\.claude\\.credentials.json: not found.',
    null,
  );
  assert.equal(missingAuth, null);
  const stale = describeClaudeFailure('Network error: timed out', usage);
  assert.equal(stale, null);
});

test('codex failures become stale-status metadata while preserving the last snapshot', () => {
  const stale = describeCodexFailure('network down', codex);
  assert.equal(stale.title, 'Showing last Codex values');
  assert.equal(stale.compactText, null);
  assert.match(stale.message, /network down/);

  const empty = describeCodexFailure('no auth', null);
  assert.equal(empty.title, 'Codex usage unavailable');
  assert.equal(empty.compactText, null);
});
