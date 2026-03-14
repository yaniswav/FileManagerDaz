/**
 * i18n module for FileManagerDaz
 *
 * Lightweight internationalization system using Svelte stores.
 * Supports French and English.
 */

import { writable, derived, get } from 'svelte/store';
import fr from './locales/fr';
import en from './locales/en';

// Supported locales
export type Locale = 'fr' | 'en';
export const SUPPORTED_LOCALES: Locale[] = ['fr', 'en'];
export const DEFAULT_LOCALE: Locale = 'en';

// Locale display names (in their own language)
export const LOCALE_NAMES: Record<Locale, string> = {
  fr: 'Francais',
  en: 'English',
};

// Translation dictionary type (flexible nested structure)
type TranslationDict = Record<string, unknown>;

// Translation dictionaries
const translations: Record<Locale, TranslationDict> = {
  fr: fr as TranslationDict,
  en: en as TranslationDict,
};

function getInitialLocale(): Locale {
  const browserLang =
    typeof navigator !== 'undefined' ? navigator.language.split('-')[0] : DEFAULT_LOCALE;
  return SUPPORTED_LOCALES.includes(browserLang as Locale) ? (browserLang as Locale) : DEFAULT_LOCALE;
}

// Current locale store (initialized from browser language)
export const locale = writable<Locale>(getInitialLocale());

// Current translations store (derived from locale)
export const translations$ = derived(locale, ($locale) => translations[$locale]);

/**
 * Get a nested value from an object using dot notation
 * @example getNestedValue({ a: { b: 'hello' } }, 'a.b') => 'hello'
 */
function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  return path.split('.').reduce((current, key) => {
    if (current && typeof current === 'object' && key in current) {
      return (current as Record<string, unknown>)[key];
    }
    return undefined;
  }, obj as unknown);
}

/**
 * Replace template variables in a string
 * @example interpolate('Hello {name}!', { name: 'World' }) => 'Hello World!'
 */
function interpolate(text: string, params?: Record<string, string | number>): string {
  if (!params) return text;

  return text.replace(/\{(\w+)\}/g, (match, key) => {
    return params[key]?.toString() ?? match;
  });
}

/**
 * Translation function - reactive store version
 * Use in components: {$t('key.path')} or {$t('key.path', { param: value })}
 */
export const t = derived(locale, ($locale) => {
  return (key: string, params?: Record<string, string | number>): string => {
    const dict = translations[$locale];
    const value = getNestedValue(dict as unknown as Record<string, unknown>, key);

    if (typeof value === 'string') {
      return interpolate(value, params);
    }

    // Fallback: try default locale
    if ($locale !== DEFAULT_LOCALE) {
      const fallbackValue = getNestedValue(
        translations[DEFAULT_LOCALE] as unknown as Record<string, unknown>,
        key
      );
      if (typeof fallbackValue === 'string') {
        return interpolate(fallbackValue, params);
      }
    }

    // Key not found - return the key itself for debugging
    console.warn(`[i18n] Missing translation: ${key}`);
    return key;
  };
});

/**
 * Non-reactive translation function for use outside components
 * @example translate('key.path', { param: value })
 */
export function translate(key: string, params?: Record<string, string | number>): string {
  const $locale = get(locale);
  const dict = translations[$locale];
  const value = getNestedValue(dict as unknown as Record<string, unknown>, key);

  if (typeof value === 'string') {
    return interpolate(value, params);
  }

  // Fallback: try default locale
  if ($locale !== DEFAULT_LOCALE) {
    const fallbackValue = getNestedValue(
      translations[DEFAULT_LOCALE] as unknown as Record<string, unknown>,
      key
    );
    if (typeof fallbackValue === 'string') {
      return interpolate(fallbackValue, params);
    }
  }

  return key;
}

/**
 * Set the current locale
 */
export function setLocale(newLocale: Locale): void {
  if (SUPPORTED_LOCALES.includes(newLocale)) {
    locale.set(newLocale);
  } else {
    console.warn(`[i18n] Unsupported locale: ${newLocale}, falling back to ${DEFAULT_LOCALE}`);
    locale.set(DEFAULT_LOCALE);
  }
  syncHtmlLang();
}

/**
 * Get the current locale (non-reactive)
 */
export function getLocale(): Locale {
  return get(locale);
}

/**
 * Initialize locale from saved settings
 */
export function initLocale(savedLocale?: string): void {
  if (savedLocale && SUPPORTED_LOCALES.includes(savedLocale as Locale)) {
    locale.set(savedLocale as Locale);
  } else {
    locale.set(getInitialLocale());
  }
  syncHtmlLang();
}

/**
 * Syncs the <html lang="..."> attribute with the current locale
 */
function syncHtmlLang(): void {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = get(locale);
  }
}

/**
 * Returns a locale string suitable for Intl APIs.
 */
export function getIntlLocale(localeValue: Locale = get(locale)): string {
  switch (localeValue) {
    case 'fr':
      return 'fr-FR';
    case 'en':
      return 'en-US';
    default:
      return 'en-US';
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function collectStringKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  const keys: string[] = [];
  for (const [key, value] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') {
      keys.push(path);
      continue;
    }
    if (isRecord(value)) {
      keys.push(...collectStringKeys(value, path));
    }
  }
  return keys;
}

function validateTranslations(): void {
  const byLocale: Record<Locale, Set<string>> = {
    fr: new Set(collectStringKeys(translations.fr)),
    en: new Set(collectStringKeys(translations.en)),
  };

  for (const base of SUPPORTED_LOCALES) {
    for (const other of SUPPORTED_LOCALES) {
      if (base === other) continue;
      const missing = [...byLocale[base]].filter((k) => !byLocale[other].has(k));
      if (missing.length > 0) {
        console.warn(`[i18n] Missing ${missing.length} keys in ${other} (present in ${base})`, missing);
      }
    }
  }
}

// Dev-only integrity checks
try {
  if (import.meta?.env?.DEV) {
    validateTranslations();
  }
} catch {
  // ignore
}
