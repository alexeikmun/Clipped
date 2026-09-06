import { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { ContentPreview } from "./components/ContentPreview";
import { ImagePreview } from "./components/ImagePreview";

interface ClipItem {
  id: string;
  text: string;
  is_favorite: boolean;
  clip_type?: "text" | "image";
  image_path?: string;
  image_width?: number;
  image_height?: number;
  ocr_text?: string;
  full_text_len?: number;
}

interface SearchResult {
  item: ClipItem;
  score: number;
}

interface AppSettings {
  shortcut: string;
}

const StarIcon = ({ filled, onClick, className }: { filled: boolean; onClick?: (e: React.MouseEvent) => void; className?: string }) => (
  <div className={className} onClick={onClick}>
    <svg 
      xmlns="http://www.w3.org/2000/svg" 
      width="16" 
      height="16" 
      viewBox="0 0 24 24" 
      fill={filled ? "#fbbf24" : "none"} 
      stroke={filled ? "#fbbf24" : "currentColor"} 
      strokeWidth="2" 
      strokeLinecap="round" 
      strokeLinejoin="round"
    >
      <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
    </svg>
  </div>
);

function App() {
  const [history, setHistory] = useState<ClipItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [isSearchVisible, setIsSearchVisible] = useState(false);
  const [showFavorites, setShowFavorites] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [shortcut, setShortcut] = useState("Ctrl+Alt+Shift+.");
  const [shortcutInput, setShortcutInput] = useState("");
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [isSavingShortcut, setIsSavingShortcut] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const shortcutInputRef = useRef<HTMLInputElement>(null);

  // Search & History sync with SQLite backend
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let active = true;

    const timer = setTimeout(() => {
      const q = searchQuery.trim();
      if (q) {
        invoke<ClipItem[]>("search_clips", { query: q, favoritesOnly: showFavorites })
          .then((items) => {
            if (active && items) setHistory(items);
          })
          .catch(console.error);
      } else {
        invoke<ClipItem[]>("get_history", { favoritesOnly: showFavorites })
          .then((items) => {
            if (active && items) setHistory(items);
          })
          .catch(console.error);
      }
    }, 20);

    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [searchQuery, showFavorites]);

  // Computed state
  const filteredItems = useMemo<SearchResult[]>(() => {
    const isTauri = "__TAURI_INTERNALS__" in window;
    if (!isTauri) {
      let items = history;
      if (showFavorites) {
        items = items.filter(item => item.is_favorite);
      }
      if (!searchQuery) {
        return items.map(item => ({ item, score: 0 }));
      }
      const lowerQuery = searchQuery.toLowerCase();
      return items
        .filter(item => (item.text + " " + (item.ocr_text || "")).toLowerCase().includes(lowerQuery))
        .map(item => ({ item, score: 0 }));
    }

    return history.map(item => ({ item, score: 0 }));
  }, [history, searchQuery, showFavorites]);

  // Keep track of latest state for event listeners
  const stateRef = useRef({ filteredItems, selectedIndex, history, isSearchVisible, searchQuery, showFavorites, showSettings, shortcut });
  stateRef.current = { filteredItems, selectedIndex, history, isSearchVisible, searchQuery, showFavorites, showSettings, shortcut };

  // Initialization effect
  useEffect(() => {
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    window.addEventListener("contextmenu", handleContextMenu);

    const isTauri = "__TAURI_INTERNALS__" in window;

    if (!isTauri) {
      console.warn("Not running in Tauri environment. APIs disabled.");
      setHistory([
        { id: "1", text: "Mock Item 1", is_favorite: false },
        { id: "2", text: "Mock Item 2", is_favorite: true },
        { id: "3", text: "Mock Item 3", is_favorite: false }
      ]);
      return;
    }

    invoke<ClipItem[]>("get_history", { favoritesOnly: false }).then((items) => {
      if (items && items.length > 0) {
        setHistory(items);
      }
    });

    invoke<AppSettings>("get_settings").then((settings) => {
      if (settings?.shortcut) {
        setShortcut(settings.shortcut);
        setShortcutInput(settings.shortcut);
      }
    });

    return () => {
      window.removeEventListener("contextmenu", handleContextMenu);
    };
  }, []);

  // Event listeners effect
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;

    const unlistenPromise = listen<ClipItem>("clipboard-new", (event) => {
      setHistory((prev) => {
        const newItem = event.payload;
        if (prev.length > 0 && prev[0].id === newItem.id) return prev;

        const { searchQuery, showFavorites } = stateRef.current;
        if (searchQuery.trim()) {
          return prev;
        }
        if (showFavorites && !newItem.is_favorite) {
          return prev;
        }
        return [newItem, ...prev].slice(0, 999);
      });
      setSelectedIndex(0);
    });

    const unlistenShortcutPromise = listen("shortcut-cycle-next", () => {
      const { history } = stateRef.current;
      setSelectedIndex((prev) => {
        if (history.length > 0) {
          const nextIndex = prev + 1;
          if (nextIndex >= history.length) {
            return 0;
          }
          return nextIndex;
        }
        return prev;
      });
    });

    const unlistenSettingsPromise = listen("open-settings", () => {
      setShowSettings(true);
      setIsSearchVisible(false);
      setSearchQuery("");
      setShortcutError(null);
      setShortcutInput(stateRef.current.shortcut);
    });

    const unlistenModalOpenedPromise = listen("modal-opened", () => {
      setShowSettings(false);
      setIsSearchVisible(false);
      setSearchQuery("");
      setShowFavorites(false);
      setSelectedIndex(0);
      invoke<ClipItem[]>("get_history", { favoritesOnly: false }).then((items) => {
        if (items) setHistory(items);
      });
    });

    return () => {
      unlistenPromise.then((f) => f());
      unlistenShortcutPromise.then((f) => f());
      unlistenSettingsPromise.then((f) => f());
      unlistenModalOpenedPromise.then((f) => f());
    };
  }, []);

  // Ensure selection is valid
  useEffect(() => {
    if (selectedIndex >= filteredItems.length && filteredItems.length > 0) {
      setSelectedIndex(Math.max(0, filteredItems.length - 1));
    }
  }, [filteredItems.length, selectedIndex]);

  // Focus input when search becomes visible
  useEffect(() => {
    if (isSearchVisible) {
      inputRef.current?.focus();
    }
  }, [isSearchVisible]);

  // Focus input on window focus if search is visible
  useEffect(() => {
    const handleFocus = () => {
      if (isSearchVisible && !showSettings) {
        inputRef.current?.focus();
      }
    };
    window.addEventListener("focus", handleFocus);
    return () => {
      window.removeEventListener("focus", handleFocus);
    };
  }, [isSearchVisible, showSettings]);

  // Scroll selected item into view
  useEffect(() => {
    const selectedEl = document.querySelector('.list-item.selected');
    if (selectedEl) {
      selectedEl.scrollIntoView({
        block: "nearest",
        behavior: "instant"
      });
    }
  }, [selectedIndex]);

  // Toggle favorite
  const toggleFavorite = async (id: string) => {
    if ("__TAURI_INTERNALS__" in window) {
      try {
        await invoke<boolean>("toggle_favorite", { id });
        setHistory(prev => {
          const updated = prev.map(item => 
            item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
          );
          return showFavorites ? updated.filter(item => item.is_favorite) : updated;
        });
      } catch (e) {
        console.error("Failed to toggle favorite:", e);
      }
    } else {
      setHistory(prev => {
        const updated = prev.map(item => 
          item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
        );
        return showFavorites ? updated.filter(item => item.is_favorite) : updated;
      });
    }
  };

  // Keyboard navigation handler
  const handleKeyDown = async (e: KeyboardEvent | React.KeyboardEvent) => {
    const { filteredItems, selectedIndex, isSearchVisible, showSettings, searchQuery } = stateRef.current;

    if (showSettings) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setShowSettings(false);
      }
      return;
    }

    if (e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      setShowFavorites(prev => !prev);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIndex((prev) => {
        const nextIndex = prev + 1;
        if (nextIndex >= filteredItems.length) return 0;
        return nextIndex;
      });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIndex((prev) => {
        const nextIndex = prev - 1;
        if (nextIndex < 0) return filteredItems.length - 1;
        return nextIndex;
      });
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      const item = filteredItems[selectedIndex]?.item;
      if (item) {
        if ("__TAURI_INTERNALS__" in window) {
          await invoke("paste_item", { text: item.text, id: item.id });
        } else {
          console.log("Mock paste:", item.text);
        }
        setIsSearchVisible(false);
        setSearchQuery("");
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();

      if (isSearchVisible || searchQuery) {
        setIsSearchVisible(false);
        setSearchQuery("");
        setSelectedIndex(0);
        return;
      }

      if ("__TAURI_INTERNALS__" in window) {
        try {
          await invoke("hide_app");
        } catch (error) {
          console.error("Failed to hide app:", error);
        }
      }
    } else if (
      !isSearchVisible &&
      e.key.length === 1 &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey
    ) {
      e.preventDefault();
      setIsSearchVisible(true);
      setSearchQuery(e.key);
      setSelectedIndex(0);
    } else if (
      isSearchVisible &&
      document.activeElement !== inputRef.current &&
      e.key.length === 1 &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey
    ) {
      e.preventDefault();
      inputRef.current?.focus();
      setSearchQuery(prev => prev + e.key);
      setSelectedIndex(0);
    } else if (
      isSearchVisible &&
      (e.key === "Backspace" || e.key === "Delete") &&
      !searchQuery
    ) {
      e.preventDefault();
      setIsSearchVisible(false);
      setSelectedIndex(0);
    } else if (
      isSearchVisible &&
      document.activeElement !== inputRef.current &&
      e.key === "Backspace"
    ) {
      e.preventDefault();
      inputRef.current?.focus();
      setSearchQuery((prev) => {
        const next = prev.slice(0, -1);
        if (!next) {
          setIsSearchVisible(false);
        }
        return next;
      });
      setSelectedIndex(0);
    } else if (
      isSearchVisible &&
      document.activeElement !== inputRef.current &&
      e.key === "Delete"
    ) {
      e.preventDefault();
      inputRef.current?.focus();
      setSearchQuery((prev) => {
        const next = prev.slice(1);
        if (!next) {
          setIsSearchVisible(false);
        }
        return next;
      });
      setSelectedIndex(0);
    }
  };

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  useEffect(() => {
    if (showSettings) {
      shortcutInputRef.current?.focus();
      shortcutInputRef.current?.select();
    }
  }, [showSettings]);

  const saveShortcut = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setShortcutError(null);
    setIsSavingShortcut(true);
    try {
      const updated = await invoke<string>("set_shortcut", { shortcut: shortcutInput });
      setShortcut(updated);
      setShortcutInput(updated);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setShortcutError(message);
    } finally {
      setIsSavingShortcut(false);
    }
  };

  const normalizeKey = (key: string) => {
    if (key === " ") return "Space";
    if (key.length === 1) return key.toUpperCase();
    const map: Record<string, string> = {
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Escape: "Esc",
      Backspace: "Backspace",
      Delete: "Delete",
      Enter: "Enter",
      Tab: "Tab",
      Home: "Home",
      End: "End",
      PageUp: "PageUp",
      PageDown: "PageDown",
      Insert: "Insert",
    };
    return map[key] ?? key;
  };

  const buildShortcutFromEvent = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Meta");

    const isModifier = e.key === "Control" || e.key === "Shift" || e.key === "Alt" || e.key === "Meta";
    if (!isModifier) {
      const key = normalizeKey(e.key);
      if (key) parts.push(key);
    }

    return parts.length > 0 && !isModifier ? parts.join("+") : "";
  };

  const renderItem = (item: ClipItem, index: number) => (
    <div 
      key={item.id}
      className={`list-item ${index === selectedIndex ? 'selected' : ''} ${item.is_favorite ? 'favorited' : ''}`}
      onClick={() => {
        setSelectedIndex(index);
        if (isSearchVisible) {
          inputRef.current?.focus();
        }
      }}
    >
      {item.is_favorite && (
        <StarIcon 
          filled={true} 
          className="star-icon-left" 
          onClick={(e) => { 
            e.stopPropagation(); 
            toggleFavorite(item.id); 
            if (isSearchVisible) {
              inputRef.current?.focus();
            }
          }} 
        />
      )}

      <div className="item-text">
        {item.clip_type === "image" ? (
          <ImagePreview
            imagePath={item.image_path}
            width={item.image_width}
            height={item.image_height}
            text={item.text}
            ocrText={item.ocr_text}
            query={searchQuery}
            isCompact={isSearchVisible}
          />
        ) : (
          <ContentPreview text={item.text} query={searchQuery} />
        )}
      </div>

      {!item.is_favorite && (
        <StarIcon 
          filled={false} 
          className="star-icon-right" 
          onClick={(e) => { 
            e.stopPropagation(); 
            toggleFavorite(item.id); 
            if (isSearchVisible) {
              inputRef.current?.focus();
            }
          }} 
        />
      )}
    </div>
  );

  return (
    <div className="app">
      <div className="card-container">
        {showSettings ? (
          <div className="header-row settings-header">
            <div className="settings-header-title">Settings</div>
          </div>
        ) : (
          <div className="header-row">
            <div className="counter-badge">
              {filteredItems.length > 0 ? selectedIndex + 1 : 0}
            </div>
            
            <div 
              onClick={() => {
                setShowFavorites(!showFavorites);
                if (isSearchVisible) {
                  inputRef.current?.focus();
                }
              }}
              style={{ 
                marginLeft: '10px', 
                cursor: 'pointer', 
                display: 'flex', 
                alignItems: 'center',
                color: showFavorites ? '#fbbf24' : '#ccc'
              }}
              title={showFavorites ? "Show all items" : "Show favorites only"}
            >
              <svg 
                xmlns="http://www.w3.org/2000/svg" 
                width="20" 
                height="20" 
                viewBox="0 0 24 24" 
                fill={showFavorites ? "currentColor" : "none"} 
                stroke="currentColor" 
                strokeWidth="2" 
                strokeLinecap="round" 
                strokeLinejoin="round"
              >
                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
              </svg>
            </div>

            {isSearchVisible && (
              <input
                ref={inputRef}
                className="search-input-visible"
                type="text"
                placeholder="Search..."
                value={searchQuery}
                onChange={(e) => {
                  const val = e.target.value;
                  setSearchQuery(val);
                  setSelectedIndex(0);
                  if (!val) {
                    setIsSearchVisible(false);
                  }
                }}
                onKeyDown={(e) => {
                  if ((e.key === "Backspace" || e.key === "Delete") && !searchQuery) {
                    e.preventDefault();
                    setIsSearchVisible(false);
                    setSelectedIndex(0);
                  }
                }}
                spellCheck={false}
                autoFocus
              />
            )}
          </div>
        )}

        <div className="card-content">
          {showSettings ? (
            <div className="settings-panel">
              <div className="settings-field">
                <div className="settings-label">Open clipboard shortcut</div>
                <input
                  ref={shortcutInputRef}
                  className="settings-input"
                  type="text"
                  value={shortcutInput}
                  readOnly
                  onKeyDown={(e) => {
                    if (e.key === "Escape") return;
                    const shortcutValue = buildShortcutFromEvent(e);
                    if (shortcutValue) {
                      e.preventDefault();
                      e.stopPropagation();
                      setShortcutInput(shortcutValue);
                    }
                  }}
                  spellCheck={false}
                  placeholder="Ctrl+Alt+Shift+."
                />
              </div>
              {shortcutError && <div className="settings-error">{shortcutError}</div>}
              <div className="settings-actions">
                <button className="settings-button" onClick={saveShortcut} disabled={isSavingShortcut}>
                  Save
                </button>
                <button className="settings-button secondary" onClick={() => setShowSettings(false)} disabled={isSavingShortcut}>
                  Back
                </button>
              </div>
              <div className="settings-hint">Current: {shortcut}</div>
            </div>
          ) : (
            filteredItems.length > 0 ? (
              (searchQuery || showFavorites) ? (
                filteredItems.map((result, index) => renderItem(result.item, index))
              ) : (
                renderItem(filteredItems[selectedIndex]?.item, selectedIndex)
              )
            ) : (
              <div style={{ padding: '20px', textAlign: 'center', color: '#888' }}>
                {history.length === 0 ? "Clipboard is empty" : "No matches found"}
              </div>
            )
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
