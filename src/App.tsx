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
        
        // Add new item to top, limit to 50
        return [newItem, ...prev].slice(0, 50);
      });
    });

    return () => {
      unlistenPromise.then((f) => f());
    };
  }, []);

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

  const handleKeyDown = async (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.min(prev + 1, filteredItems.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.max(prev - 1, 0));
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

  return (
    <div className="app" onKeyDown={handleKeyDown}>
      <div className="search-container">
        <input
          ref={inputRef}
          className="search-input"
          type="text"
          placeholder="Type to search..."
          value={searchQuery}
          onChange={(e) => {
             setSearchQuery(e.target.value);
             setSelectedIndex(0);
          }}
          autoFocus
          spellCheck={false}
        />
      </div>
      <ul className="list-container">
        {filteredItems.map((item, index) => (
          <li
            key={index}
            className={`list-item ${index === selectedIndex ? "selected" : ""}`}
            onClick={() => {
                setSelectedIndex(index);
                if ("__TAURI_INTERNALS__" in window) {
                   invoke("paste_item", { text: item });
                } else {
                   console.log("Mock paste:", item);
                }
                setSearchQuery("");
            }}
          >
            {item}
          </li>
        ))}
        {filteredItems.length === 0 && (
            <li className="list-item" style={{color: '#666', cursor: 'default'}}>
                {history.length === 0 ? "No clipboard history" : "No matches"}
            </li>
        )}
      </ul>
    </div>
  );
}

export default App;
