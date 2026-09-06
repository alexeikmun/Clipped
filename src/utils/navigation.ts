/**
 * Navigation and selection helpers for clipboard history keyboard shortcuts (1-9)
 * and selection persistence.
 */

export const STORAGE_KEY_LAST_SELECTED = "last_selected_clip_id";

/**
 * Returns true if the key is a number from '1' to '9'.
 */
export function isNumberKey(key: string): boolean {
  return /^[1-9]$/.test(key);
}

/**
 * Maps a number key '1'-'9' to a 0-based index.
 * If the index is within the range [0, totalVisible), returns the index.
 * If the index is out of range, returns null (do nothing).
 */
export function selectClipByNumber(key: string, totalVisible: number): number | null {
  if (!isNumberKey(key)) return null;
  const targetIndex = Number(key) - 1;
  if (targetIndex >= 0 && targetIndex < totalVisible) {
    return targetIndex;
  }
  return null;
}

/**
 * Determines whether a keydown event should be handled as a 1-9 number shortcut.
 * Rejects events with modifier keys (Ctrl/Meta/Alt) or when focus is inside a text input.
 */
export function shouldHandleNumberKey(
  e: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean },
  isInputFocused: boolean
): boolean {
  if (isInputFocused) return false;
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  return isNumberKey(e.key);
}

/**
 * Restores the selected index based on the persisted clip ID.
 * Gracefully falls back to 0 if the clip no longer exists (e.g. deleted or pruned).
 */
export function getRestoredSelectedIndex(
  items: { id: string }[],
  savedClipId: string | null
): number {
  if (!savedClipId || items.length === 0) return 0;
  const idx = items.findIndex((item) => item.id === savedClipId);
  return idx >= 0 ? idx : 0;
}

/**
 * Preserves the selected index on the currently selected clip when new clipboard items
 * are added/bumped to the top of the list.
 */
export function getNextSelectedIndexOnNewClip(
  currentSelectedId: string | null,
  newItems: { id: string }[]
): number {
  if (!currentSelectedId || newItems.length === 0) return 0;
  const idx = newItems.findIndex((item) => item.id === currentSelectedId);
  return idx >= 0 ? idx : 0;
}

/**
 * Safely persists the selected clip ID to localStorage.
 */
export function persistSelectedClipId(clipId: string): void {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY_LAST_SELECTED, clipId);
    }
  } catch {
    // Ignore storage errors (e.g. restricted sandbox)
  }
}

/**
 * Safely reads the persisted clip ID from localStorage.
 */
export function getPersistedSelectedClipId(): string | null {
  try {
    if (typeof localStorage !== "undefined") {
      return localStorage.getItem(STORAGE_KEY_LAST_SELECTED);
    }
  } catch {
    // Ignore storage errors
  }
  return null;
}
