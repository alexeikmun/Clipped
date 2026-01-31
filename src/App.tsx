import { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Fuse from "fuse.js";
import "./App.css";

interface SearchResult {
  item: string;
  matches?: readonly [number, number][];
}

const HighlightedText = ({ text, matches, query }: { text: string; matches?: readonly [number, number][]; query: string }) => {
  if (!matches || matches.length === 0) {
    return <span>{text}</span>;
  }

  // Merge overlapping or adjacent matches
  const sortedMatches = [...matches].sort((a, b) => a[0] - b[0]);
  const mergedMatches: [number, number][] = [];

  if (sortedMatches.length > 0) {
    let currentMatch = sortedMatches[0];
    
    for (let i = 1; i < sortedMatches.length; i++) {
      const nextMatch = sortedMatches[i];
      // Check for overlap or adjacency (start <= end + 1)
      if (nextMatch[0] <= currentMatch[1] + 1) {
        // Merge
        currentMatch = [currentMatch[0], Math.max(currentMatch[1], nextMatch[1])];
      } else {
        mergedMatches.push(currentMatch);
        currentMatch = nextMatch;
      }
    }
    mergedMatches.push(currentMatch);
  }

  const result = [];
  let lastIndex = 0;

  mergedMatches.forEach((match, i) => {
    const [start, end] = match;
    // Add text before match
    if (start > lastIndex) {
      result.push(<span key={`text-${i}`}>{text.substring(lastIndex, start)}</span>);
    }
    // Check if it's an exact match (length based) - strictly this logic might be fuzzy now but keeps basic style
    const isExact = (end - start + 1) === query.length;
    // Add highlighted match
    result.push(
      <span key={`match-${i}`} className={isExact ? "highlight exact" : "highlight"}>
        {text.substring(start, end + 1)}
      </span>
    );
    lastIndex = end + 1;
  });

  // Add remaining text
  if (lastIndex < text.length) {
    result.push(<span key="text-end">{text.substring(lastIndex)}</span>);
  }

  return <>{result}</>;
};

function App() {
  const [history, setHistory] = useState<string[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [isSearchVisible, setIsSearchVisible] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Computed state
  const filteredItems = useMemo<SearchResult[]>(() => {
    if (!searchQuery) {
      return history.map(item => ({ item }));
    }

    const fuse = new Fuse(history, {
      includeScore: true,
      includeMatches: true,
      threshold: 0.4,
      ignoreLocation: true,
      useExtendedSearch: true,
      minMatchCharLength: 2,
    });

    const results = fuse.search(searchQuery);
    return results.map(result => ({
      item: result.item,
      matches: result.matches?.[0]?.indices as readonly [number, number][] | undefined
    }));
  }, [history, searchQuery]);

  // Keep track of latest state for event listeners
  const stateRef = useRef({ filteredItems, selectedIndex, history, isSearchVisible, searchQuery });
  stateRef.current = { filteredItems, selectedIndex, history, isSearchVisible, searchQuery };

  // Initialization effect
  useEffect(() => {
    // Check if running in Tauri environment
    const isTauri = "__TAURI_INTERNALS__" in window;

    if (!isTauri) {
      console.warn("Not running in Tauri environment. APIs disabled.");
      // Mock data for preview
      setHistory(["Mock Item 1", "Mock Item 2", "Mock Item 3"]);
      return;
    }

    // Sync initial state
    invoke<string[]>("get_history").then((history) => {
      if (history && history.length > 0) {
        setHistory(history);
      }
    });
  }, []); // Run once on mount

  // Event listeners effect
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;

    const unlistenPromise = listen<string>("clipboard-new", (event) => {
      setHistory((prev) => {
        const newItem = event.payload;
        // Ignore consecutive duplicates
        if (prev.length > 0 && prev[0] === newItem) return prev;

        // Add new item to top, limit to 999
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

  // Keyboard navigation

  // Keyboard navigation handler
  const handleKeyDown = async (e: KeyboardEvent | React.KeyboardEvent) => {
    const { filteredItems, selectedIndex, isSearchVisible } = stateRef.current;

    // Use pure key values
    if (e.key === "ArrowDown") {
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
          await invoke("paste_item", { text: item });
        } else {
          console.log("Mock paste:", item);
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
    } else if (e.key === "f" && !isSearchVisible) {
      e.preventDefault();
      setIsSearchVisible(true);
    }
  };

  // Attach global listener for when input is not focused
  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []); // Run once


  return (
    <div className="app">
      <div className="card-container">
        <div className="header-row">
          <div className="counter-badge">
            {filteredItems.length > 0 ? selectedIndex + 1 : 0}
          </div>
          
          <div 
            onClick={() => {
              if (isSearchVisible) {
                // Optional: Toggle off or just focus? 
                // Let's toggle off to act as a close button if needed, 
                // but user asked to "display searchbar". 
                // If it's already visible, maybe just focus it.
                // But typically icons toggle. Let's try toggle.
                setIsSearchVisible(false);
                setSearchQuery("");
              } else {
                setIsSearchVisible(true);
              }
            }}
            style={{ 
              marginLeft: '10px', 
              cursor: 'pointer', 
              display: 'flex', 
              alignItems: 'center',
              opacity: isSearchVisible ? 0.7 : 1 // Dim slightly if active
            }}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ color: '#ccc' }}>
              <circle cx="11" cy="11" r="8"></circle>
              <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
            </svg>
          </div>

          {isSearchVisible && (
            <input
              ref={inputRef}
              onKeyDown={handleKeyDown}
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
            searchQuery ? (
              filteredItems.map((result, index) => (
                <div 
                  key={index}
                  className={`list-item ${index === selectedIndex ? 'selected' : ''}`}
                  onClick={() => setSelectedIndex(index)}
                >
                  <HighlightedText text={result.item} matches={result.matches} query={searchQuery} />
                </div>
              ))
            ) : (
              <div className="list-item selected" style={{ cursor: 'default' }}>
                {filteredItems[selectedIndex]?.item}
              </div>
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
