import { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { ContentPreview } from "./components/ContentPreview";

interface ClipItem {
  id: string;
  text: string;
  is_favorite: boolean;
}

interface SearchResult {
  item: ClipItem;
  score: number;
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
  const inputRef = useRef<HTMLInputElement>(null);

  // Computed state
  const filteredItems = useMemo<SearchResult[]>(() => {
    let items = history;
    
    if (showFavorites) {
      items = items.filter(item => item.is_favorite);
    }

    if (!searchQuery) {
      return items.map(item => ({ item, score: 0 }));
    }

    const lowerQuery = searchQuery.toLowerCase();
    const terms = lowerQuery.split(/\s+/).filter(t => t.length > 0);

    if (terms.length === 0) {
      return items.map(item => ({ item, score: 0 }));
    }

    const results: SearchResult[] = [];

    items.forEach(item => {
      const lowerText = item.text.toLowerCase();
      
      // Strict AND matching: all terms must be present
      const allTermsMatch = terms.every(term => lowerText.includes(term));
      
      if (allTermsMatch) {
        // Calculate a simple score for sorting
        // 1. Exact match (highest)
        // 2. Starts with query (high)
        // 3. Contains all terms (base)
        // 4. Term proximity (optional, maybe later)
        
        let score = 1;
        if (lowerText === lowerQuery) score += 100;
        else if (lowerText.startsWith(lowerQuery)) score += 50;
        
        // Boost if terms are close to each other or in order?
        // For now, just recency (preserved by loop order) + prefix match is good enough.
        
        results.push({ item, score });
      }
    });

    return results;
  }, [history, searchQuery, showFavorites]);

  // Keep track of latest state for event listeners
  const stateRef = useRef({ filteredItems, selectedIndex, history, isSearchVisible, searchQuery, showFavorites });
  stateRef.current = { filteredItems, selectedIndex, history, isSearchVisible, searchQuery, showFavorites };

  // Initialization effect
  useEffect(() => {
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    window.addEventListener("contextmenu", handleContextMenu);

    // Check if running in Tauri environment
    const isTauri = "__TAURI_INTERNALS__" in window;

    if (!isTauri) {
      console.warn("Not running in Tauri environment. APIs disabled.");
      // Mock data for preview
      setHistory([
        { id: "1", text: "Mock Item 1", is_favorite: false },
        { id: "2", text: "Mock Item 2", is_favorite: true },
        { id: "3", text: "Mock Item 3", is_favorite: false }
      ]);
      return;
    }

    // Sync initial state
    invoke<ClipItem[]>("get_history").then((history) => {
      if (history && history.length > 0) {
        setHistory(history);
      }
    });
    return () => {
      window.removeEventListener("contextmenu", handleContextMenu);
    };
  }, []); // Run once on mount

  // Event listeners effect
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;

    // Listen for new items (single item or list update? The backend emits "clipboard-new" with the new item)
    // BUT we need to update the whole list or prepend. 
    // The backend emits ClipItem.
    const unlistenPromise = listen<ClipItem>("clipboard-new", (event) => {
      setHistory((prev) => {
        const newItem = event.payload;
        // Ignore consecutive duplicates (check ID or text)
        if (prev.length > 0 && prev[0].text === newItem.text) return prev;

        // Add new item to top, limit to 999 (backend handles truncation logic for persistence, but we should sync)
        // Actually, easiest is to fetch history again or trust the event.
        // If we append, we might drift from backend logic (smart truncation).
        // But for UI responsiveness, we append.
        // We should respect the "exclude favorites from rotation" logic if we were implementing it here,
        // but backend handles it.
        return [newItem, ...prev].slice(0, 999);
      });
      // Reset selection to top when new item arrives
      setSelectedIndex(0);
    });

    // Listen for shortcut cycle event
    const unlistenShortcutPromise = listen("shortcut-cycle-next", () => {
      console.log("Shortcut cycle event received"); // Debug log
      const { history } = stateRef.current;

      setSelectedIndex((prev) => {
        // If we have history, cycle to the next item
        if (history.length > 0) {
          const nextIndex = prev + 1;
          // Cycle back to 0 if we reach the end
          if (nextIndex >= history.length) {
            return 0;
          }
          const newIndex = nextIndex;
          console.log("Cycling from", prev, "to", newIndex); // Debug log
          return newIndex;
        }
        return prev;
      });
    });

    return () => {
      unlistenPromise.then((f) => f());
      unlistenShortcutPromise.then((f) => f());
    };
  }, []); // Run once on mount

  // Ensure selection is valid
  useEffect(() => {
    if (selectedIndex >= filteredItems.length && filteredItems.length > 0) {
      setSelectedIndex(Math.max(0, filteredItems.length - 1));
    }
  }, [filteredItems.length, selectedIndex]);

  // Focus input on window focus
  useEffect(() => {
    const handleFocus = () => {
      inputRef.current?.focus();
    };
    window.addEventListener("focus", handleFocus);
    inputRef.current?.focus();

    return () => {
      window.removeEventListener("focus", handleFocus);
    };
  }, []);

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
        const newHistory = await invoke<ClipItem[]>("toggle_favorite", { id });
        setHistory(newHistory);
      } catch (e) {
        console.error("Failed to toggle favorite:", e);
      }
    } else {
      // Mock toggle
      setHistory(prev => prev.map(item => 
        item.id === id ? { ...item, is_favorite: !item.is_favorite } : item
      ));
    }
  };

  // Keyboard navigation handler
  const handleKeyDown = async (e: KeyboardEvent | React.KeyboardEvent) => {
    const { filteredItems, selectedIndex, isSearchVisible } = stateRef.current;

    // Use pure key values
    if (e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      setShowFavorites(prev => !prev);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIndex((prev) => {
        const nextIndex = prev + 1;
        if (nextIndex >= filteredItems.length) return 0; // Cycle to start
        return nextIndex;
      });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIndex((prev) => {
        const nextIndex = prev - 1;
        if (nextIndex < 0) return filteredItems.length - 1; // Cycle to end
        return nextIndex;
      });
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      const item = filteredItems[selectedIndex]?.item;
      if (item) {
        if ("__TAURI_INTERNALS__" in window) {
          await invoke("paste_item", { text: item.text });
        } else {
          console.log("Mock paste:", item.text);
        }
        setSearchQuery(""); // Clear search on paste
      }
    } else if (e.key === "Escape") {
      console.log("Escape key pressed");
      e.preventDefault();
      e.stopPropagation();

      // Clear search first
      setSearchQuery("");

      if (isSearchVisible) {
        setIsSearchVisible(false);
        return;
      }

      if ("__TAURI_INTERNALS__" in window) {
        try {
          console.log("Invoking hide_app...");
          await invoke("hide_app");
          console.log("hide_app invoked successfully");
        } catch (error) {
          console.error("Failed to hide app:", error);
        }
      } else {
        console.warn("Not in Tauri, cannot hide window");
      }
    } else if (
      (!isSearchVisible || document.activeElement !== inputRef.current) &&
      e.key.length === 1 &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey
    ) {
      e.preventDefault();
      setIsSearchVisible(true);
      setSearchQuery(e.key);
      setSelectedIndex(0);
      inputRef.current?.focus();
    }
  };

  // Attach global listener for when input is not focused
  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []); // Run once


  const renderItem = (item: ClipItem, index: number) => (
    <div 
      key={item.id}
      className={`list-item ${index === selectedIndex ? 'selected' : ''} ${item.is_favorite ? 'favorited' : ''}`}
      onClick={() => setSelectedIndex(index)}
    >
      {/* Left Star (only if favorited) */}
      {item.is_favorite && (
        <StarIcon 
          filled={true} 
          className="star-icon-left" 
          onClick={(e) => { 
            e.stopPropagation(); 
            toggleFavorite(item.id); 
          }} 
        />
      )}

      <div className="item-text">
        <ContentPreview text={item.text} query={searchQuery} />
      </div>

      {/* Right Star (only if NOT favorited) */}
      {!item.is_favorite && (
        <StarIcon 
          filled={false} 
          className="star-icon-right" 
          onClick={(e) => { 
            e.stopPropagation(); 
            toggleFavorite(item.id); 
          }} 
        />
      )}
    </div>
  );

  return (
    <div className="app">
      <div className="card-container">
        <div className="header-row">
          <div className="counter-badge">
            {filteredItems.length > 0 ? selectedIndex + 1 : 0}
          </div>
          
          <div 
            onClick={() => setShowFavorites(!showFavorites)}
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
                setSearchQuery(e.target.value);
                setSelectedIndex(0);
              }}
              spellCheck={false}
              autoFocus
            />
          )}
        </div>

        <div className="card-content">
          {filteredItems.length > 0 ? (
            (searchQuery || showFavorites) ? (
              filteredItems.map((result, index) => renderItem(result.item, index))
            ) : (
              // Even in single item view, we use renderItem to show stars
              renderItem(filteredItems[selectedIndex]?.item, selectedIndex)
            )
          ) : (
            <div style={{ padding: '20px', textAlign: 'center', color: '#888' }}>
              {history.length === 0 ? "Clipboard is empty" : "No matches found"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
