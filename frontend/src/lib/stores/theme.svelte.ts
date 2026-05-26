/**
 * Theme store — toggles the `.dark` class on <html>.
 *
 * Two themes today: 'paper' (light, default) and 'warmth' (dark warm).
 * Persisted to localStorage under `harness-theme`. The initial value is
 * applied via an inline script in `app.html` to avoid a flash before
 * Svelte hydrates.
 */

export type ThemeName = 'paper' | 'warmth';

const STORAGE_KEY = 'harness-theme';

function readInitial(): ThemeName {
  if (typeof window === 'undefined') return 'paper';
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'warmth' || stored === 'dark') return 'warmth';
  } catch {
    // ignore — fall back to paper
  }
  return 'paper';
}

function apply(theme: ThemeName) {
  if (typeof document === 'undefined') return;
  const html = document.documentElement;
  if (theme === 'warmth') html.classList.add('dark');
  else html.classList.remove('dark');
}

class ThemeStore {
  current = $state<ThemeName>(readInitial());

  constructor() {
    if (typeof document !== 'undefined') apply(this.current);
  }

  set(theme: ThemeName) {
    this.current = theme;
    apply(theme);
    try {
      localStorage.setItem(STORAGE_KEY, theme === 'warmth' ? 'dark' : 'paper');
    } catch {
      // best-effort
    }
  }

  toggle() {
    this.set(this.current === 'paper' ? 'warmth' : 'paper');
  }
}

export const theme = new ThemeStore();
