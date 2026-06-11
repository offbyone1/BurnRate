import { readFileSync } from 'node:fs';
import { strict as assert } from 'node:assert';
import test from 'node:test';

test('desktop widget window stays out of the Windows taskbar', () => {
  const raw = readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8');
  const config = JSON.parse(raw);
  const mainWindow = config.app.windows.find((windowConfig: { label?: string }) => windowConfig.label === 'main');

  assert.equal(mainWindow.skipTaskbar, true);
});
