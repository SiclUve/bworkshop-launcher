import { test } from 'node:test';
import assert from 'node:assert/strict';
import { translateError } from './errors.ts';

test('français : inchangé (sauf le préfixe Error:)', () => {
  assert.equal(translateError('fr', "Error: Cet outil est déjà lancé."), 'Cet outil est déjà lancé.');
});

test('anglais : message composé traduit morceau par morceau', () => {
  assert.equal(
    translateError('en', "Impossible de lancer l'outil : erreur réseau : délai dépassé"),
    'Could not start the tool: network error: délai dépassé',
  );
  assert.equal(translateError('en', 'Cet outil est déjà lancé.'), 'This tool is already running.');
});
