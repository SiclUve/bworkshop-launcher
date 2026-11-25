// Lancer avec : npm test (lanceur de tests intégré à Node, sans dépendance).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { editDistance, fuzzyScore, normalize, search } from './fuzzy.ts';

test('normalisation : accents et casse ignorés', () => {
  assert.equal(normalize('  Éléphant Forgé '), 'elephant forge');
});

test('distance avec transposition', () => {
  assert.equal(editDistance('hrae', 'hare'), 1);
  assert.equal(editDistance('forge', 'forge'), 0);
  assert.equal(editDistance('frge', 'forge'), 1);
});

test('« hare » trouve le Simulateur Harebourg', () => {
  assert.ok(fuzzyScore('hare', 'Simulateur Harebourg') >= 90);
});

test('fautes de frappe tolérées', () => {
  assert.ok(fuzzyScore('hrae', 'Simulateur Harebourg') > 0);
  assert.ok(fuzzyScore('frogé', 'Assistant Forge') > 0);
  assert.ok(fuzzyScore('grimiore', 'Grimoire Boss') > 0);
});

test('pas de faux positifs sur des mots trop différents', () => {
  assert.equal(fuzzyScore('xyz', 'Assistant Forge'), 0);
  assert.equal(fuzzyScore('lingo', 'Simulateur Harebourg'), 0);
});

test('recherche triée par pertinence', () => {
  const tools = ['Grimoire', 'Grimoire Boss', 'Assistant Forge', 'Simulateur Harebourg'];
  assert.deepEqual(search('grim', tools, (t) => [t]), ['Grimoire', 'Grimoire Boss']);
  assert.deepEqual(search('', tools, (t) => [t]), tools);
  assert.deepEqual(search('hare', tools, (t) => [t]), ['Simulateur Harebourg']);
});
