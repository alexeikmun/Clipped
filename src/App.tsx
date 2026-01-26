import { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

function App() {
  const [history, setHistory] = useState<string[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

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

    const unlistenPromise = listen<string>("clipboard-new", (event) => {
      setHistory((prev) => {
        const newItem = event.payload;
        // Ignore consecutive duplicates
        if (prev.length > 0 && prev[0] === newItem) return prev;
        
        // Add new item to top, limit to 999
        return [newItem, ...prev].slice(0, 999);
      });
    });

    // Listen for shortcut cycle event
    const unlistenShortcutPromise = listen("shortcut-cycle-next", () => {
      console.log("Shortcut cycle event received"); // Debug log
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
  }, [history]); // Add history dependency to access latest length for shortcut cycle

  const filteredItems = useMemo(() => {
    if (!searchQuery) return history;
    const lowerQuery = searchQuery.toLowerCase();
    return history.filter((item) => item.toLowerCase().includes(lowerQuery));
  }, [history, searchQuery]);

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

  const handleKeyDown = async (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => {
        const nextIndex = prev + 1;
        if (nextIndex >= filteredItems.length) return 0; // Cycle to start
        return nextIndex;
      });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => {
        const nextIndex = prev - 1;
        if (nextIndex < 0) return filteredItems.length - 1; // Cycle to end
        return nextIndex;
      });
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = filteredItems[selectedIndex];
      if (item) {
        if ("__TAURI_INTERNALS__" in window) {
           await invoke("paste_item", { text: item });
        } else {
           console.log("Mock paste:", item);
        }
        setSearchQuery(""); // Clear search on paste
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      if ("__TAURI_INTERNALS__" in window) {
        await getCurrentWindow().hide();
        await invoke("set_monitoring", { monitoring: true });
      }
      setSearchQuery(""); // Clear search on close
    }
  };

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [filteredItems, selectedIndex]); // Re-attach listener when state changes

  return (
    <div className="app">
      {filteredItems.length > 0 ? (
        <div className="card-container">
          <div className="header-row">
            <div className="counter-badge">
               {selectedIndex + 1}
            </div>
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
            />
          </div>
          <div className="card-content">
             {filteredItems[selectedIndex]}
          </div>
        </div>
      ) : (
         <div className="card-container empty">
             <div className="header-row">
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
                  autoFocus
                  spellCheck={false}
                />
             </div>
             <div className="card-content">
                {history.length === 0 ? "No history" : "No matches"}
             </div>
         </div>
      )}
    </div>
  );
}

export default App;
