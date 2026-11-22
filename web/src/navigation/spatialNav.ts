export type TvNavKey =
  | 'ArrowUp'
  | 'ArrowDown'
  | 'ArrowLeft'
  | 'ArrowRight'
  | 'Select'
  | 'SelectLong'
  | 'Back'
  | 'Menu';

function getActiveScope(): HTMLElement {
  const modals = Array.from(document.querySelectorAll<HTMLElement>('[data-tv-modal="true"]'));
  if (modals.length > 0) {
    return modals[modals.length - 1];
  }
  return document.body;
}

interface FocusCandidate {
  el: HTMLElement;
  rect: DOMRect;
}

function getFocusCandidates(scope?: HTMLElement): FocusCandidate[] {
  const root = scope ?? getActiveScope();
  const nodes = root.querySelectorAll<HTMLElement>('[data-tv-focusable="true"]');
  const list: FocusCandidate[] = [];
  for (let i = 0; i < nodes.length; i++) {
    const el = nodes[i];
    if (el.hasAttribute('disabled') || el.getAttribute('aria-disabled') === 'true') continue;
    const rect = el.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      list.push({ el, rect });
    }
  }
  return list;
}

export function getFocusables(scope?: HTMLElement): HTMLElement[] {
  return getFocusCandidates(scope).map((c) => c.el);
}

export function focusElement(el: HTMLElement | null): void {
  if (!el) return;
  el.focus({ preventScroll: true });

  const modalBody = el.closest<HTMLElement>('.tv-modal-body');
  if (modalBody) {
    const bodyRect = modalBody.getBoundingClientRect();
    const targetBlock = el.closest<HTMLElement>('.tv-control-box, .tv-custom-section') ?? el;
    const elRect = el.getBoundingClientRect();
    const blockRect = targetBlock.getBoundingClientRect();
    const margin = 16;

    // Prefer keeping the focused element visible with generous breathing room
    if (elRect.top < bodyRect.top + margin) {
      const topTarget = blockRect.height < bodyRect.height * 0.65 ? blockRect.top : elRect.top;
      modalBody.scrollTop = Math.max(0, modalBody.scrollTop + (topTarget - (bodyRect.top + margin)));
    } else if (elRect.bottom > bodyRect.bottom - margin) {
      modalBody.scrollTop += elRect.bottom - (bodyRect.bottom - margin);
    }
    return;
  }

  const scrollRoot = el.closest<HTMLElement>('.tv-scroll-root');
  if (scrollRoot) {
    // Always reveal the very top cleanly when focusing anything in the status bar or hero
    if (el.closest('.tv-status-bar') || el.closest('.tv-hero')) {
      scrollRoot.scrollTop = 0;
    } else {
      // If inside a shelf, include the shelf header in the vertical scroll bounds
      const targetBlock = el.closest<HTMLElement>('.tv-shelf') ?? el;
      const rootRect = scrollRoot.getBoundingClientRect();
      const targetRect = targetBlock.getBoundingClientRect();
      const topMargin = 20;
      const bottomMargin = 24;

      if (targetRect.top < rootRect.top + topMargin) {
        const delta = targetRect.top - (rootRect.top + topMargin);
        scrollRoot.scrollTop = Math.max(0, scrollRoot.scrollTop + delta);
      } else if (targetRect.bottom > rootRect.bottom - bottomMargin) {
        const delta = targetRect.bottom - (rootRect.bottom - bottomMargin);
        scrollRoot.scrollTop += delta;
      }
    }
  }

  // Horizontal scroll inside shelf track if needed
  const track = el.closest<HTMLElement>('.tv-shelf-track');
  if (track) {
    const trackRect = track.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const sideMargin = 16;
    if (elRect.left < trackRect.left + sideMargin) {
      track.scrollLeft = Math.max(0, track.scrollLeft + (elRect.left - (trackRect.left + sideMargin)));
    } else if (elRect.right > trackRect.right - sideMargin) {
      track.scrollLeft += elRect.right - (trackRect.right - sideMargin);
    }
  } else if (!scrollRoot) {
    el.scrollIntoView({ behavior: 'auto', block: 'nearest', inline: 'nearest' });
  }
}

export function focusFirstInScope(scope?: HTMLElement): void {
  const list = getFocusables(scope);
  if (list.length > 0) {
    focusElement(list[0]);
  }
}

function pickBestInList(
  current: HTMLElement,
  candidates: FocusCandidate[],
  direction: 'ArrowUp' | 'ArrowDown' | 'ArrowLeft' | 'ArrowRight',
): HTMLElement | null {
  if (candidates.length === 0) return null;

  const currRect = current.getBoundingClientRect();
  const cx = currRect.left + currRect.width / 2;
  const cy = currRect.top + currRect.height / 2;

  let best: HTMLElement | null = null;
  let bestScore = Number.POSITIVE_INFINITY;

  for (let i = 0; i < candidates.length; i++) {
    const { el, rect: r } = candidates[i];
    if (el === current) continue;
    const tx = r.left + r.width / 2;
    const ty = r.top + r.height / 2;
    const dx = tx - cx;
    const dy = ty - cy;

    let primary = 0;
    let secondary = 0;

    if (direction === 'ArrowRight') {
      if (dx <= 6) continue;
      secondary = cy >= r.top && cy <= r.bottom ? 0 : Math.min(Math.abs(cy - r.top), Math.abs(cy - r.bottom));
      // Never jump diagonally to another row when pressing Right
      if (secondary > Math.max(currRect.height, r.height) * 0.75) continue;
      primary = Math.max(0, r.left - currRect.right) + dx * 0.15;
    } else if (direction === 'ArrowLeft') {
      if (dx >= -6) continue;
      secondary = cy >= r.top && cy <= r.bottom ? 0 : Math.min(Math.abs(cy - r.top), Math.abs(cy - r.bottom));
      // Never jump diagonally to another row when pressing Left
      if (secondary > Math.max(currRect.height, r.height) * 0.75) continue;
      primary = Math.max(0, currRect.left - r.right) + -dx * 0.15;
    } else if (direction === 'ArrowDown') {
      if (dy <= 6) continue;
      primary = Math.max(0, r.top - currRect.bottom) + dy * 0.15;
      secondary = cx >= r.left && cx <= r.right ? 0 : Math.min(Math.abs(cx - r.left), Math.abs(cx - r.right));
    } else {
      if (dy >= -6) continue;
      primary = Math.max(0, currRect.top - r.bottom) + -dy * 0.15;
      secondary = cx >= r.left && cx <= r.right ? 0 : Math.min(Math.abs(cx - r.left), Math.abs(cx - r.right));
    }

    const sameRowBonus =
      (direction === 'ArrowLeft' || direction === 'ArrowRight') && secondary < currRect.height * 0.75
        ? 0.35
        : 1;
    const score = (primary + secondary * 2.4) * sameRowBonus;

    if (score < bestScore) {
      bestScore = score;
      best = el;
    }
  }

  return best;
}

function findNextFocusable(
  current: HTMLElement,
  direction: 'ArrowUp' | 'ArrowDown' | 'ArrowLeft' | 'ArrowRight',
): HTMLElement | null {
  // Keep Left/Right strictly inside the top dock when focused on top bar icons
  const statusDock = current.closest<HTMLElement>('.tv-status-bar');
  if (statusDock && (direction === 'ArrowLeft' || direction === 'ArrowRight')) {
    return pickBestInList(current, getFocusCandidates(statusDock), direction);
  }

  const modalBody = current.closest<HTMLElement>('.tv-modal-body');
  if (modalBody && (direction === 'ArrowDown' || direction === 'ArrowUp')) {
    const innerHit = pickBestInList(current, getFocusCandidates(modalBody), direction);
    if (innerHit) return innerHit;
  }

  return pickBestInList(current, getFocusCandidates(), direction);
}

export function installSpatialNavigation(handlers: {
  onBack: () => void;
  onMenu: () => void;
}): () => void {
  const handleNav = (key: TvNavKey) => {
    const active = document.activeElement as HTMLElement | null;
    const isTyping =
      active &&
      (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA') &&
      (active as HTMLInputElement).type !== 'range' &&
      (active as HTMLInputElement).type !== 'checkbox';

    if (key === 'Back') {
      if (isTyping) {
        active.blur();
        return;
      }
      handlers.onBack();
      return;
    }

    if (key === 'Menu') {
      if (active?.dataset.tvFocusable === 'true') {
        const ev = new CustomEvent('tv-contextmenu', { bubbles: true, cancelable: true });
        if (!active.dispatchEvent(ev)) {
          return;
        }
      }
      handlers.onMenu();
      return;
    }

    if (isTyping && key !== 'ArrowDown' && key !== 'ArrowUp') {
      return;
    }

    const scope = getActiveScope();
    const current =
      active && scope.contains(active) && active.dataset.tvFocusable === 'true' ? active : null;

    if (key === 'Select') {
      if (current) {
        current.click();
      } else {
        focusFirstInScope(scope);
      }
      return;
    }

    if (key === 'SelectLong') {
      if (current) {
        current.dispatchEvent(new CustomEvent('tv-contextmenu', { bubbles: true, cancelable: true }));
      }
      return;
    }

    if (!current) {
      focusFirstInScope(scope);
      return;
    }

    const next = findNextFocusable(current, key);
    if (next) {
      focusElement(next);
    }
  };

  window.__tvKeyDown = (rawKey: string) => {
    handleNav(rawKey as TvNavKey);
  };

  const onWindowKeyDown = (e: KeyboardEvent) => {
    const active = document.activeElement as HTMLElement | null;
    const isTyping =
      active &&
      (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA') &&
      (active as HTMLInputElement).type !== 'range' &&
      (active as HTMLInputElement).type !== 'checkbox';

    if (isTyping) {
      if (e.key === 'Escape') {
        e.preventDefault();
        active.blur();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowUp':
      case 'ArrowDown':
      case 'ArrowLeft':
      case 'ArrowRight':
        e.preventDefault();
        handleNav(e.key);
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        handleNav('Select');
        break;
      case 'Escape':
      case 'Backspace':
        e.preventDefault();
        handleNav('Back');
        break;
      case 'm':
      case 'M':
      case 'ContextMenu':
        e.preventDefault();
        handleNav('Menu');
        break;
    }
  };

  window.addEventListener('keydown', onWindowKeyDown);
  return () => {
    window.removeEventListener('keydown', onWindowKeyDown);
    delete window.__tvKeyDown;
  };
}
