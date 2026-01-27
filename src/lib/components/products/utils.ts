export const KNOWN_CONTENT_TYPES = [
  'character',
  'clothing',
  'hair',
  'prop',
  'environment',
  'pose',
  'light',
  'material',
  'script',
  'morph',
  'hdri',
  'other',
] as const;

export type KnownContentType = (typeof KNOWN_CONTENT_TYPES)[number];

export function normalizeContentType(value: string | null): KnownContentType | null {
  if (!value) return null;
  const lower = value.toLowerCase();
  return (KNOWN_CONTENT_TYPES as readonly string[]).includes(lower) ? (lower as KnownContentType) : null;
}

export function getContentTypeIcon(value: string | null): string {
  const type = normalizeContentType(value);
  switch (type) {
    case 'character':
      return 'CHR';
    case 'clothing':
      return 'CLO';
    case 'hair':
      return 'HAI';
    case 'prop':
      return 'PRP';
    case 'environment':
      return 'ENV';
    case 'pose':
      return 'POS';
    case 'light':
      return 'LGT';
    case 'material':
      return 'MAT';
    case 'script':
      return 'SCR';
    case 'morph':
      return 'MOR';
    case 'hdri':
      return 'HDR';
    case 'other':
      return 'OTH';
    default:
      return 'OTH';
  }
}
