import { ClipItem, renderTextPreview, renderImagePreview, copyOcrText, escapeHtml } from './preview';
import { starIconSvg, trashIconSvg, headerStarIconSvg } from './icons';

export interface AppState {
  history: ClipItem[];
  filteredItems: ClipItem[];
  selectedIndex: number;
  searchQuery: string;
  isSearchVisible: boolean;
  showFavorites: boolean;
  showSettings: boolean;
  shortcut: string;
  shortcutInput: string;
  shortcutError: string | null;
  isSavingShortcut: boolean;
}

export interface AppCallbacks {
  onSelectIndex: (index: number) => void;
  onToggleFavorite: (id: string) => void;
  onDeleteClip: (id: string) => void;
  onToggleFavoritesFilter: () => void;
  onSearchInput: (value: string) => void;
  onSearchClose: () => void;
  onSaveShortcut: () => void;
  onCloseSettings: () => void;
  onSetShortcutInput: (val: string) => void;
}

export class AppView {
  private root: HTMLElement;
  private callbacks: AppCallbacks;
  private searchInputEl: HTMLInputElement | null = null;

  constructor(root: HTMLElement, callbacks: AppCallbacks) {
    this.root = root;
    this.callbacks = callbacks;
    this.initLayout();
    this.attachEvents();
  }

  private initLayout(): void {
    this.root.innerHTML = `
      <div class="app">
        <div class="card-container">
          <div id="header-mount" class="header-row"></div>
          <div id="content-mount" class="card-content"></div>
          <div id="card-actions-dock" class="card-actions-dock"></div>
        </div>
      </div>
    `;
  }

  private attachEvents(): void {
    this.root.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;

      // Copy OCR button
      const copyBtn = target.closest<HTMLButtonElement>('[data-action="copy-ocr"]');
      if (copyBtn) {
        e.stopPropagation();
        const text = copyBtn.getAttribute('data-text') || '';
        copyOcrText(copyBtn, text);
        return;
      }

      // Delete clip button
      const deleteBtn = target.closest<HTMLElement>('[data-action="delete"]');
      if (deleteBtn) {
        e.stopPropagation();
        const id = deleteBtn.getAttribute('data-id');
        if (id) {
          this.callbacks.onDeleteClip(id);
          this.focusSearchIfNeeded();
        }
        return;
      }

      // Star favorite button
      const starBtn = target.closest<HTMLElement>('[data-action="favorite"]');
      if (starBtn) {
        e.stopPropagation();
        const id = starBtn.getAttribute('data-id');
        if (id) {
          this.callbacks.onToggleFavorite(id);
          this.focusSearchIfNeeded();
        }
        return;
      }

      // Header favorite toggle
      const headerFav = target.closest<HTMLElement>('#header-fav-toggle');
      if (headerFav) {
        this.callbacks.onToggleFavoritesFilter();
        this.focusSearchIfNeeded();
        return;
      }

      // Settings Save button
      const saveBtn = target.closest<HTMLElement>('#settings-save-btn');
      if (saveBtn) {
        this.callbacks.onSaveShortcut();
        return;
      }

      // Settings Back button
      const backBtn = target.closest<HTMLElement>('#settings-back-btn');
      if (backBtn) {
        this.callbacks.onCloseSettings();
        return;
      }

      // List item click
      const listItem = target.closest<HTMLElement>('.list-item');
      if (listItem) {
        const indexStr = listItem.getAttribute('data-index');
        if (indexStr !== null) {
          this.callbacks.onSelectIndex(parseInt(indexStr, 10));
          this.focusSearchIfNeeded();
        }
      }
    });
  }

  public render(state: AppState): void {
    const headerMount = this.root.querySelector<HTMLElement>('#header-mount');
    const contentMount = this.root.querySelector<HTMLElement>('#content-mount');
    if (!headerMount || !contentMount) return;

    // 1. Render Header
    if (state.showSettings) {
      headerMount.className = 'header-row settings-header';
      headerMount.innerHTML = `<div class="settings-header-title">Settings</div>`;
    } else {
      headerMount.className = 'header-row';
      const counterText = state.filteredItems.length > 0 ? (state.selectedIndex + 1).toString() : '0';
      headerMount.innerHTML = `
        <div class="counter-badge">${counterText}</div>
        <div 
          id="header-fav-toggle" 
          style="margin-left: 10px; cursor: pointer; display: flex; align-items: center; color: ${state.showFavorites ? '#fbbf24' : '#ccc'};"
          title="${state.showFavorites ? 'Show all items' : 'Show favorites only'}"
        >
          ${headerStarIconSvg(state.showFavorites)}
        </div>
        ${state.isSearchVisible ? `
          <input 
            id="search-input" 
            class="search-input-visible" 
            type="text" 
            placeholder="Search..." 
            value="${escapeHtml(state.searchQuery)}" 
            spellcheck="false" 
            autocomplete="off"
          />
        ` : ''}
      `;

      if (state.isSearchVisible) {
        const searchInput = headerMount.querySelector<HTMLInputElement>('#search-input');
        if (searchInput) {
          this.searchInputEl = searchInput;
          searchInput.addEventListener('input', (e) => {
            const target = e.target as HTMLInputElement;
            this.callbacks.onSearchInput(target.value);
          });
          searchInput.addEventListener('keydown', (e) => {
            if (e.key === 'Backspace' && !state.searchQuery) {
              e.preventDefault();
              this.callbacks.onSearchClose();
            }
          });
        }
      } else {
        this.searchInputEl = null;
      }
    }

    // 2. Render Content
    if (state.showSettings) {
      const actionsDock = this.root.querySelector<HTMLElement>('#card-actions-dock');
      if (actionsDock) actionsDock.innerHTML = '';
      contentMount.innerHTML = `
        <div class="settings-panel">
          <div class="settings-field">
            <div class="settings-label">Open clipboard shortcut</div>
            <input
              id="shortcut-input"
              class="settings-input"
              type="text"
              value="${escapeHtml(state.shortcutInput)}"
              readonly
              spellcheck="false"
              placeholder="Ctrl+Alt+Shift+."
            />
          </div>
          ${state.shortcutError ? `<div class="settings-error">${escapeHtml(state.shortcutError)}</div>` : ''}
          <div class="settings-actions">
            <button id="settings-save-btn" class="settings-button" ${state.isSavingShortcut ? 'disabled' : ''}>
              Save
            </button>
            <button id="settings-back-btn" class="settings-button secondary" ${state.isSavingShortcut ? 'disabled' : ''}>
              Back
            </button>
          </div>
          <div class="settings-hint">Current: ${escapeHtml(state.shortcut)}</div>
        </div>
      `;

      const shortcutInput = contentMount.querySelector<HTMLInputElement>('#shortcut-input');
      if (shortcutInput) {
        shortcutInput.addEventListener('keydown', (e) => {
          if (e.key === 'Escape') return;
          const combo = this.buildShortcutFromEvent(e);
          if (combo) {
            e.preventDefault();
            e.stopPropagation();
            this.callbacks.onSetShortcutInput(combo);
          }
        });
        shortcutInput.focus();
        shortcutInput.select();
      }
    } else {
      const actionsDock = this.root.querySelector<HTMLElement>('#card-actions-dock');

      if (state.filteredItems.length === 0) {
        if (actionsDock) actionsDock.innerHTML = '';
        contentMount.innerHTML = `
          <div style="padding: 20px; text-align: center; color: #888;">
            ${state.history.length === 0 ? 'Clipboard is empty' : 'No matches found'}
          </div>
        `;
      } else {
        const selected = state.filteredItems[state.selectedIndex] || state.filteredItems[0];

        if (state.searchQuery || state.showFavorites) {
          // Multi-item list view
          const itemsHtml = state.filteredItems
            .map((item, idx) => this.renderListItemHtml(item, idx, state.selectedIndex, state.searchQuery, true))
            .join('');
          contentMount.innerHTML = itemsHtml;
        } else {
          // Single item card view
          contentMount.innerHTML = selected 
            ? this.renderListItemHtml(selected, state.selectedIndex, state.selectedIndex, '', false)
            : '';
        }

        if (actionsDock) {
          actionsDock.innerHTML = selected ? `
            <div class="action-icon trash-icon" data-action="delete" data-id="${selected.id}" title="Delete clip (Del)">${trashIconSvg()}</div>
            <div class="action-divider"></div>
            <div class="action-icon star-icon ${selected.is_favorite ? 'favorited' : ''}" data-action="favorite" data-id="${selected.id}" title="${selected.is_favorite ? 'Remove favorite' : 'Add to favorites'}">${starIconSvg(selected.is_favorite)}</div>
          ` : '';
        }
      }
    }

    // 3. Scroll active item into view
    this.scrollSelectedIntoView();
  }

  private renderListItemHtml(
    item: ClipItem,
    index: number,
    selectedIndex: number,
    query: string,
    isCompact: boolean
  ): string {
    const isSelected = index === selectedIndex;
    const itemPreview = item.clip_type === 'image'
      ? renderImagePreview(item, query, isCompact)
      : renderTextPreview(item.text, query);

    const modeClass = isCompact ? 'compact' : 'single-card';
    return `<div class="list-item ${modeClass} ${isSelected ? 'selected' : ''} ${item.is_favorite ? 'favorited' : ''}" data-index="${index}" data-id="${item.id}"><div class="item-text">${itemPreview}</div></div>`;
  }

  public focusSearchIfNeeded(): void {
    if (this.searchInputEl && document.activeElement !== this.searchInputEl) {
      this.searchInputEl.focus();
    }
  }

  public blurSearch(): void {
    this.searchInputEl?.blur();
  }

  public isSearchFocused(): boolean {
    return document.activeElement === this.searchInputEl;
  }

  public scrollSelectedIntoView(): void {
    const selectedEl = this.root.querySelector('.list-item.selected');
    if (selectedEl) {
      selectedEl.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    }
  }

  private buildShortcutFromEvent(e: KeyboardEvent): string {
    const parts: string[] = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Meta');

    const isModifier = e.key === 'Control' || e.key === 'Shift' || e.key === 'Alt' || e.key === 'Meta';
    if (!isModifier) {
      const map: Record<string, string> = {
        ' ': 'Space',
        ArrowUp: 'Up',
        ArrowDown: 'Down',
        ArrowLeft: 'Left',
        ArrowRight: 'Right',
        Escape: 'Esc',
        Backspace: 'Backspace',
        Delete: 'Delete',
        Enter: 'Enter',
        Tab: 'Tab',
        Home: 'Home',
        End: 'End',
        PageUp: 'PageUp',
        PageDown: 'PageDown',
        Insert: 'Insert',
      };
      const normKey = map[e.key] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
      if (normKey) parts.push(normKey);
    }

    return parts.length > 0 && !isModifier ? parts.join('+') : '';
  }
}
