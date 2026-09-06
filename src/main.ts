import './App.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ClipItem } from './ui/preview';
import { AppView, AppState } from './ui/appView';
import {
  selectClipByNumber,
  shouldHandleNumberKey,
  getRestoredSelectedIndex,
  persistSelectedClipId,
  getPersistedSelectedClipId,
} from './utils/navigation';

interface AppSettings {
  shortcut: string;
}

const isTauri = '__TAURI_INTERNALS__' in window;

const state: AppState = {
  history: [],
  filteredItems: [],
  selectedIndex: 0,
  searchQuery: '',
  isSearchVisible: false,
  showFavorites: false,
  showSettings: false,
  shortcut: 'Ctrl+Alt+Shift+.',
  shortcutInput: '',
  shortcutError: null,
  isSavingShortcut: false,
};

let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

const rootEl = document.getElementById('root')!;

const appView = new AppView(rootEl, {
  onSelectIndex: (index: number) => {
    state.selectedIndex = index;
    persistCurrentSelection();
    appView.render(state);
  },
  onToggleFavorite: (id: string) => {
    toggleFavorite(id);
  },
  onDeleteClip: (id: string) => {
    deleteClip(id);
  },
  onToggleFavoritesFilter: () => {
    state.showFavorites = !state.showFavorites;
    syncFilteredItems();
  },
  onSearchInput: (value: string) => {
    state.searchQuery = value;
    state.selectedIndex = 0;
    if (!value) {
      state.isSearchVisible = false;
    }
    syncFilteredItems();
  },
  onSearchClose: () => {
    state.isSearchVisible = false;
    state.searchQuery = '';
    state.selectedIndex = 0;
    syncFilteredItems();
  },
  onSaveShortcut: () => {
    saveShortcut();
  },
  onCloseSettings: () => {
    state.showSettings = false;
    appView.render(state);
  },
  onSetShortcutInput: (val: string) => {
    state.shortcutInput = val;
    appView.render(state);
  },
});

function persistCurrentSelection(): void {
  const current = state.filteredItems[state.selectedIndex];
  if (current) {
    persistSelectedClipId(current.id);
  }
}

function syncFilteredItems(): void {
  if (!isTauri) {
    let items = state.history;
    if (state.showFavorites) {
      items = items.filter(item => item.is_favorite);
    }
    if (state.searchQuery) {
      const lower = state.searchQuery.toLowerCase();
      items = items.filter(item => (item.text + ' ' + (item.ocr_text || '')).toLowerCase().includes(lower));
    }
    state.filteredItems = items;
    clampSelectedIndex();
    appView.render(state);
    return;
  }

  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer);
  }

  searchDebounceTimer = setTimeout(() => {
    const q = state.searchQuery.trim();
    if (q) {
      invoke<ClipItem[]>('search_clips', { query: q, favoritesOnly: state.showFavorites })
        .then((items) => {
          if (items) {
            state.filteredItems = items;
            clampSelectedIndex();
            appView.render(state);
          }
        })
        .catch(console.error);
    } else {
      invoke<ClipItem[]>('get_history', { favoritesOnly: state.showFavorites })
        .then((items) => {
          if (items) {
            state.filteredItems = items;
            clampSelectedIndex();
            appView.render(state);
          }
        })
        .catch(console.error);
    }
  }, 20);
}

function clampSelectedIndex(): void {
  if (state.selectedIndex >= state.filteredItems.length && state.filteredItems.length > 0) {
    state.selectedIndex = Math.max(0, state.filteredItems.length - 1);
  }
}

async function toggleFavorite(id: string): Promise<void> {
  if (isTauri) {
    try {
      await invoke<boolean>('toggle_favorite', { id });
      state.history = state.history.map(item =>
        item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
      );
      state.filteredItems = state.filteredItems.map(item =>
        item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
      );
      if (state.showFavorites) {
        state.filteredItems = state.filteredItems.filter(item => item.is_favorite);
      }
      clampSelectedIndex();
      persistCurrentSelection();
      appView.render(state);
    } catch (e) {
      console.error('Failed to toggle favorite:', e);
    }
  } else {
    state.history = state.history.map(item =>
      item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
    );
    state.filteredItems = state.filteredItems.map(item =>
      item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
    );
    if (state.showFavorites) {
      state.filteredItems = state.filteredItems.filter(item => item.is_favorite);
    }
    clampSelectedIndex();
    persistCurrentSelection();
    appView.render(state);
  }
}

async function deleteClip(id: string): Promise<void> {
  if (isTauri) {
    try {
      await invoke<boolean>('delete_clip', { id });
    } catch (e) {
      console.error('Failed to delete clip:', e);
      return;
    }
  }

  state.history = state.history.filter(item => item.id !== id);
  state.filteredItems = state.filteredItems.filter(item => item.id !== id);
  state.selectedIndex = state.selectedIndex > 0 ? state.selectedIndex - 1 : 0;
  persistCurrentSelection();
  appView.render(state);
}

async function pasteSelectedItem(): Promise<void> {
  const item = state.filteredItems[state.selectedIndex];
  if (!item) return;

  if (isTauri) {
    await invoke('paste_item', { text: item.text, id: item.id });
  } else {
    console.log('Mock paste:', item.text);
  }

  state.isSearchVisible = false;
  state.searchQuery = '';
  appView.render(state);
}

async function saveShortcut(): Promise<void> {
  if (!isTauri) return;
  state.shortcutError = null;
  state.isSavingShortcut = true;
  appView.render(state);

  try {
    const updated = await invoke<string>('set_shortcut', { shortcut: state.shortcutInput });
    state.shortcut = updated;
    state.shortcutInput = updated;
  } catch (error) {
    state.shortcutError = error instanceof Error ? error.message : String(error);
  } finally {
    state.isSavingShortcut = false;
    appView.render(state);
  }
}

// Global keyboard events
window.addEventListener('keydown', async (e: KeyboardEvent) => {
  if (state.showSettings) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      state.showSettings = false;
      appView.render(state);
    }
    return;
  }

  const activeTag = document.activeElement?.tagName.toLowerCase();
  const isInputFocused = activeTag === 'input' || activeTag === 'textarea';

  // 1-9 Number key navigation
  if (shouldHandleNumberKey(e, isInputFocused)) {
    e.preventDefault();
    e.stopPropagation();
    const targetIndex = selectClipByNumber(e.key, state.filteredItems.length);
    if (targetIndex !== null) {
      state.selectedIndex = targetIndex;
      persistCurrentSelection();
      appView.render(state);
    }
    return;
  }

  // Delete key to remove clip
  if (e.key === 'Delete' && !e.ctrlKey && !e.altKey && !e.metaKey) {
    if (!isInputFocused || !state.searchQuery) {
      e.preventDefault();
      e.stopPropagation();
      const current = state.filteredItems[state.selectedIndex];
      if (current) {
        deleteClip(current.id);
      }
      return;
    }
  }

  if (e.key === 'Tab') {
    e.preventDefault();
    e.stopPropagation();
    state.showFavorites = !state.showFavorites;
    syncFilteredItems();
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    e.stopPropagation();
    appView.blurSearch();
    if (state.filteredItems.length > 0) {
      state.selectedIndex = (state.selectedIndex + 1) % state.filteredItems.length;
      persistCurrentSelection();
      appView.render(state);
    }
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    e.stopPropagation();
    appView.blurSearch();
    if (state.filteredItems.length > 0) {
      state.selectedIndex = state.selectedIndex - 1 < 0 ? state.filteredItems.length - 1 : state.selectedIndex - 1;
      persistCurrentSelection();
      appView.render(state);
    }
  } else if (e.key === 'Enter') {
    e.preventDefault();
    e.stopPropagation();
    pasteSelectedItem();
  } else if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();

    if (state.isSearchVisible || state.searchQuery) {
      state.isSearchVisible = false;
      state.searchQuery = '';
      state.selectedIndex = 0;
      syncFilteredItems();
      return;
    }

    if (isTauri) {
      try {
        await invoke('hide_app');
      } catch (error) {
        console.error('Failed to hide app:', error);
      }
    }
  } else if (
    !state.isSearchVisible &&
    e.key.length === 1 &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey
  ) {
    e.preventDefault();
    state.isSearchVisible = true;
    state.searchQuery = e.key;
    state.selectedIndex = 0;
    syncFilteredItems();
    setTimeout(() => appView.focusSearchIfNeeded(), 0);
  } else if (
    state.isSearchVisible &&
    !appView.isSearchFocused() &&
    e.key.length === 1 &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey
  ) {
    e.preventDefault();
    state.searchQuery += e.key;
    state.selectedIndex = 0;
    syncFilteredItems();
    setTimeout(() => appView.focusSearchIfNeeded(), 0);
  } else if (
    state.isSearchVisible &&
    !appView.isSearchFocused() &&
    e.key === 'Backspace'
  ) {
    e.preventDefault();
    state.searchQuery = state.searchQuery.slice(0, -1);
    if (!state.searchQuery) {
      state.isSearchVisible = false;
    }
    state.selectedIndex = 0;
    syncFilteredItems();
    if (state.isSearchVisible) {
      setTimeout(() => appView.focusSearchIfNeeded(), 0);
    }
  }
});

// Window focus/contextmenu
window.addEventListener('contextmenu', (e) => e.preventDefault());
window.addEventListener('focus', () => {
  if (state.isSearchVisible && !state.showSettings) {
    appView.focusSearchIfNeeded();
  }
});

// Initialization
async function init(): Promise<void> {
  const savedClipId = getPersistedSelectedClipId();

  if (!isTauri) {
    console.warn('Not running in Tauri environment. APIs disabled.');
    const mockItems: ClipItem[] = [
      { id: '1', text: 'Mock Item 1', is_favorite: false },
      { id: '2', text: 'Mock Item 2', is_favorite: true },
      { id: '3', text: 'Mock Item 3', is_favorite: false },
    ];
    state.history = mockItems;
    state.filteredItems = mockItems;
    state.selectedIndex = getRestoredSelectedIndex(mockItems, savedClipId);
    appView.render(state);
    return;
  }

  try {
    const items = await invoke<ClipItem[]>('get_history', { favoritesOnly: false });
    if (items && items.length > 0) {
      state.history = items;
      state.filteredItems = items;
      state.selectedIndex = getRestoredSelectedIndex(items, savedClipId);
    }

    const settings = await invoke<AppSettings>('get_settings');
    if (settings?.shortcut) {
      state.shortcut = settings.shortcut;
      state.shortcutInput = settings.shortcut;
    }
  } catch (err) {
    console.error('Initialization error:', err);
  }

  appView.render(state);

  // Tauri listeners
  listen<ClipItem>('clipboard-new', (event) => {
    const newItem = event.payload;
    if (state.history.length > 0 && state.history[0].id === newItem.id) return;

    const withoutExisting = state.history.filter(item => item.id !== newItem.id);
    if (state.searchQuery.trim()) {
      return;
    }
    if (state.showFavorites && !newItem.is_favorite) {
      state.history = withoutExisting;
      state.filteredItems = withoutExisting.filter(item => item.is_favorite);
      clampSelectedIndex();
      appView.render(state);
      return;
    }

    const updated = [newItem, ...withoutExisting].slice(0, 999);
    state.history = updated;
    state.filteredItems = updated;
    state.selectedIndex = 0;
    persistSelectedClipId(newItem.id);
    appView.render(state);
  });

  listen('shortcut-cycle-next', () => {
    if (state.history.length > 0) {
      state.selectedIndex = (state.selectedIndex + 1) % state.history.length;
      persistCurrentSelection();
      appView.render(state);
    }
  });

  listen('open-settings', () => {
    state.showSettings = true;
    state.isSearchVisible = false;
    state.searchQuery = '';
    state.shortcutError = null;
    state.shortcutInput = state.shortcut;
    appView.render(state);
  });

  listen('modal-opened', async () => {
    state.showSettings = false;
    state.isSearchVisible = false;
    state.searchQuery = '';
    state.showFavorites = false;

    const savedId = getPersistedSelectedClipId();
    try {
      const items = await invoke<ClipItem[]>('get_history', { favoritesOnly: false });
      if (items) {
        state.history = items;
        state.filteredItems = items;
        state.selectedIndex = getRestoredSelectedIndex(items, savedId);
      }
    } catch (e) {
      console.error('Failed to reload history on modal-opened:', e);
    }

    appView.render(state);
  });
}

init();
