/**
 * Command palette action registry types & helpers.
 *
 * Each action is a self-contained operation invoked from the palette
 * (Ctrl+Shift+P). Parent components own the closures so the palette
 * stays stateless.
 */

export type PaletteGroup = 'Navigate' | 'View' | 'Actions';

export interface PaletteAction {
  id: string;
  label: string;
  icon?: string;
  group: PaletteGroup;
  action: () => void | Promise<void>;
}

/** Substring match on label and id, case-insensitive. */
export function filterActions(query: string, actions: PaletteAction[]): PaletteAction[] {
  const q = query.trim().toLowerCase();
  if (!q) return actions;
  return actions.filter(
    (a) => a.label.toLowerCase().includes(q) || a.id.toLowerCase().includes(q)
  );
}
