// Lancer avec : npm test (lanceur de tests intégré à Node, sans dépendance).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { hotkeyLabel } from './i18n.ts';

test('raccourci affiché : symboles Mac, texte inchangé ailleurs', () => {
  assert.equal(hotkeyLabel('Ctrl+Alt+B', false), 'Ctrl+Alt+B');
  assert.equal(hotkeyLabel('Ctrl+Alt+B', true), '⌃⌥B');
  assert.equal(hotkeyLabel('Win+Shift+F2', true), '⌘⇧F2');
});
