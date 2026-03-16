/**
 * Theme Engine for FileManagerDaz
 *
 * Manages 4 theme states: light, dark, oled, system
 * Persists to localStorage, injects data-theme on <html>
 */

export type Theme = 'light' | 'dark' | 'oled' | 'system';
export const THEMES: Theme[] = ['light', 'system', 'dark', 'oled'];

export const THEME_LABELS: Record<Theme, string> = {
  light: 'Light',
  system: 'System',
  dark: 'Dark',
  oled: 'OLED',
};

const STORAGE_KEY = 'fmdaz-theme';

function getSystemPrefersDark(): boolean {
  if (typeof window === 'undefined') return true;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolveTheme(theme: Theme): 'light' | 'dark' | 'oled' {
  if (theme === 'system') return getSystemPrefersDark() ? 'dark' : 'light';
  return theme;
}

function loadSaved(): Theme {
  if (typeof localStorage === 'undefined') return 'dark';
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && THEMES.includes(saved as Theme)) return saved as Theme;
  return 'dark';
}

// ── Reactive state ──
let current: Theme = $state(loadSaved());
let resolved: 'light' | 'dark' | 'oled' = $state(resolveTheme(loadSaved()));
let mediaQuery: MediaQueryList | null = null;
let mediaHandler: ((e: MediaQueryListEvent) => void) | null = null;

function applyTheme() {
  resolved = resolveTheme(current);
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', resolved);
    // Update color-scheme for native elements
    document.documentElement.style.colorScheme = resolved === 'light' ? 'light' : 'dark';
  }
}

function onSystemChange(e: MediaQueryListEvent) {
  if (current === 'system') {
    applyTheme();
  }
}

/** Initialize theme engine — call once from +page.svelte onMount */
export function initTheme(): void {
  applyTheme();

  // Listen for system theme changes
  if (typeof window !== 'undefined') {
    mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaHandler = onSystemChange;
    mediaQuery.addEventListener('change', mediaHandler);
  }
}

/** Clean up listeners */
export function destroyTheme(): void {
  if (mediaQuery && mediaHandler) {
    mediaQuery.removeEventListener('change', mediaHandler);
    mediaQuery = null;
    mediaHandler = null;
  }
}

/** Set the theme */
export function setTheme(theme: Theme): void {
  current = theme;
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, theme);
  }
  applyTheme();
}

/** Get current user-chosen theme (reactive) */
export function getTheme(): Theme {
  return current;
}

/** Get the resolved theme (light/dark/oled, accounting for system) */
export function getResolvedTheme(): 'light' | 'dark' | 'oled' {
  return resolved;
}

// Export reactive getters for Svelte 5 components
export const themeState = {
  get current() { return current; },
  get resolved() { return resolved; },
};
