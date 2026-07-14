import { strict as assert } from 'node:assert';
import test from 'node:test';
import type { ClaudeUsageResponse, CodexUsage } from '../src/types.ts';
import { renderCompact } from '../src/ui.ts';

class FakeClassList {
  private values = new Set<string>();

  add(...names: string[]): void {
    for (const name of names) this.values.add(name);
  }

  remove(...names: string[]): void {
    for (const name of names) this.values.delete(name);
  }

  contains(name: string): boolean {
    return this.values.has(name);
  }

  toggle(name: string, force?: boolean): boolean {
    const next = force ?? !this.values.has(name);
    if (next) this.values.add(name);
    else this.values.delete(name);
    return next;
  }
}

class FakeElement {
  textContent = '';
  innerHTML = '';
  style: Record<string, string> = {};
  classList = new FakeClassList();
  private attributes = new Set<string>();

  setAttribute(name: string, _value: string): void {
    this.attributes.add(name);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  hasAttribute(name: string): boolean {
    return this.attributes.has(name);
  }
}

function setupCompactDom(): Record<string, FakeElement> {
  const ids = [
    'compact-view',
    'five-hour-compact',
    'seven-day-compact',
    'five-hour-label',
    'seven-day-label',
    'pill-row-logo-primary',
    'pill-row-logo-secondary',
    'pill-row-secondary',
    'five-hour-compact-2',
    'seven-day-compact-2',
    'five-hour-label-2',
    'seven-day-label-2',
  ];
  const elements = Object.fromEntries(ids.map((id) => [id, new FakeElement()])) as Record<string, FakeElement>;
  elements['pill-row-logo-primary'].setAttribute('hidden', '');
  elements['pill-row-logo-secondary'].setAttribute('hidden', '');
  elements['pill-row-secondary'].setAttribute('hidden', '');

  globalThis.document = {
    getElementById(id: string) {
      return elements[id] ?? null;
    },
  } as unknown as Document;

  return elements;
}

const claudeUsage: ClaudeUsageResponse = {
  five_hour: { utilization: 42, resets_at: null },
  seven_day: { utilization: 7, resets_at: null },
  extra_usage: null,
};

const codexUsage: CodexUsage = {
  planType: 'pro',
  primary: { utilization: 21, windowMinutes: 300, resetsAt: null },
  secondary: { utilization: 4, windowMinutes: 10080, resetsAt: null },
  snapshotAt: '2026-05-25T12:00:00.000Z',
};

test('single Claude compact pill shows Claude logo instead of a generic product logo', () => {
  const elements = setupCompactDom();

  renderCompact(claudeUsage, null, { claude: true, codex: false });

  assert.equal(elements['pill-row-logo-primary'].hasAttribute('hidden'), false);
  assert.match(elements['pill-row-logo-primary'].innerHTML, /D97757/);
  assert.equal(elements['pill-row-logo-secondary'].hasAttribute('hidden'), true);
  assert.equal(elements['compact-view'].classList.contains('provider-logo-mode'), true);
});

test('single Codex compact pill shows Codex logo instead of a generic product logo', () => {
  const elements = setupCompactDom();

  renderCompact({ five_hour: null, seven_day: null, extra_usage: null }, codexUsage, {
    claude: false,
    codex: true,
  });

  assert.equal(elements['pill-row-logo-primary'].hasAttribute('hidden'), false);
  assert.match(elements['pill-row-logo-primary'].innerHTML, /codex-gradient/);
  assert.equal(elements['pill-row-logo-secondary'].hasAttribute('hidden'), true);
  assert.equal(elements['compact-view'].classList.contains('provider-logo-mode'), true);
});

test('single Codex compact pill displays used quota', () => {
  const elements = setupCompactDom();

  renderCompact({ five_hour: null, seven_day: null, extra_usage: null }, codexUsage, {
    claude: false,
    codex: true,
  });

  assert.equal(elements['five-hour-compact'].textContent, '21%');
  assert.equal(elements['seven-day-compact'].textContent, '4%');
});

// Codex removed its 5h window: the weekly window now arrives as `primary`.
// It must render in the 7d slot, leaving the 5h slot empty — not slide into 5h.
test('Codex weekly-only usage renders in the 7d slot, 5h slot empty', () => {
  const elements = setupCompactDom();
  const weeklyOnly: CodexUsage = {
    planType: 'pro',
    primary: { utilization: 74, windowMinutes: 10080, resetsAt: null },
    secondary: null,
    snapshotAt: '2026-05-25T12:00:00.000Z',
  };

  renderCompact({ five_hour: null, seven_day: null, extra_usage: null }, weeklyOnly, {
    claude: false,
    codex: true,
  });

  assert.equal(elements['five-hour-compact'].textContent, '--');
  assert.equal(elements['seven-day-compact'].textContent, '74%');
});
