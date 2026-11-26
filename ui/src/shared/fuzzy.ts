// Recherche tolérante : ignore accents et casse, accepte une ou deux fautes de frappe.
// Utilisée par la recherche de la fenêtre principale.

/** Minuscules, sans accents, espaces en trop retirés. */
export function normalize(s: string): string {
  return s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .trim();
}

/** Distance d'édition avec transpositions (« hrae » → « hare » = 1). */
export function editDistance(a: string, b: string): number {
  const rows = a.length + 1;
  const cols = b.length + 1;
  const d: number[][] = Array.from({ length: rows }, (_, i) =>
    Array.from({ length: cols }, (_, j) => (i === 0 ? j : j === 0 ? i : 0)),
  );
  for (let i = 1; i < rows; i++) {
    for (let j = 1; j < cols; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
      }
    }
  }
  return d[a.length][b.length];
}

function isSubsequence(q: string, t: string): boolean {
  let i = 0;
  for (const c of t) if (c === q[i]) i++;
  return i === q.length;
}

/** Fautes tolérées selon la longueur de la recherche. */
function allowedTypos(length: number): number {
  if (length <= 3) return 0;
  if (length <= 6) return 1;
  return 2;
}

/** Score de correspondance : 0 = aucune, plus c'est haut, mieux c'est. */
export function fuzzyScore(query: string, text: string): number {
  const q = normalize(query);
  const t = normalize(text);
  if (!q) return 1;
  if (!t) return 0;
  if (t.startsWith(q)) return 100;
  const words = t.split(/[\s\-_'’.]+/).filter(Boolean);
  if (words.some((w) => w.startsWith(q))) return 90;
  if (t.includes(q)) return 80;
  if (q.length >= 3 && isSubsequence(q, t)) return 60;
  const max = allowedTypos(q.length);
  if (max === 0) return 0;
  let best = Infinity;
  for (const w of words) {
    best = Math.min(best, editDistance(q, w.slice(0, q.length)), editDistance(q, w));
  }
  return best <= max ? 50 - best * 10 : 0;
}

/** Filtre et trie une liste ; à score égal, l'ordre d'origine est gardé. */
export function search<T>(query: string, items: T[], fields: (item: T) => string[]): T[] {
  if (!normalize(query)) return items;
  return items
    .map((item, index) => ({
      item,
      index,
      score: Math.max(0, ...fields(item).map((f, k) => fuzzyScore(query, f) - k * 5)),
    }))
    .filter((r) => r.score > 0)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((r) => r.item);
}
