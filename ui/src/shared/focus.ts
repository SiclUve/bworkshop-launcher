// Garde le focus clavier dans une boîte de dialogue modale (Tab et Maj+Tab bouclent sur ses
// éléments), et le rend à l'élément d'origine à la fermeture.

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function trapFocus(node: HTMLElement) {
  const previous = document.activeElement as HTMLElement | null;
  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !node.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !node.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }
  document.addEventListener('keydown', onKey);
  return {
    destroy() {
      document.removeEventListener('keydown', onKey);
      previous?.focus?.();
    },
  };
}
