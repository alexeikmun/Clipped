import { useState, useEffect } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { HighlightedText } from "./HighlightedText";

interface ImagePreviewProps {
  imagePath?: string;
  width?: number;
  height?: number;
  text?: string;
  ocrText?: string;
  query?: string;
  isCompact?: boolean;
}

export const ImagePreview = ({
  imagePath,
  width,
  height,
  text,
  ocrText,
  query = "",
  isCompact = false,
}: ImagePreviewProps) => {
  const [src, setSrc] = useState<string>("");
  const [hasError, setHasError] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    setHasError(false);
    if (!imagePath) {
      setSrc("");
      return;
    }

    try {
      if ("__TAURI_INTERNALS__" in window) {
        setSrc(convertFileSrc(imagePath));
      } else {
        setSrc(imagePath);
      }
    } catch {
      setHasError(true);
    }
  }, [imagePath]);

  const handleCopyText = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!ocrText) return;

    try {
      if ("__TAURI_INTERNALS__" in window) {
        await invoke("copy_text", { text: ocrText });
      } else if (navigator.clipboard) {
        await navigator.clipboard.writeText(ocrText);
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (err) {
      console.error("Failed to copy text:", err);
    }
  };

  const dimensionText = width && height ? `${width} × ${height}` : "Image";

  if (hasError || !src) {
    return (
      <div className={`clip-image-fallback ${isCompact ? "compact" : ""}`}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
          <circle cx="8.5" cy="8.5" r="1.5"></circle>
          <polyline points="21 15 16 10 5 21"></polyline>
        </svg>
        <span>{text || `[${dimensionText}]`}</span>
        {ocrText && (
          <div className="clip-ocr-snippet">
            <HighlightedText text={ocrText} query={query} />
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={`clip-image-preview ${isCompact ? "compact" : ""}`}>
      <div className="preview-image-wrapper">
        <img
          src={src}
          alt={text || "Clipboard Image"}
          className={`preview-image ${isCompact ? "compact" : ""}`}
          onError={() => setHasError(true)}
          loading="lazy"
        />
      </div>

      <div className="clip-image-meta">
        <span className="clip-image-badge">{dimensionText}</span>
        {ocrText && (
          <div className="clip-ocr-actions">
            <span className="clip-ocr-badge">OCR</span>
            <button
              type="button"
              className={`clip-ocr-copy-btn ${copied ? "copied" : ""}`}
              onClick={handleCopyText}
              title="Copy extracted text to clipboard"
            >
              {copied ? (
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="#4ade80"
                  strokeWidth="2.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <polyline points="20 6 9 17 4 12"></polyline>
                </svg>
              ) : (
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
              )}
              <span>{copied ? "Copied" : "Copy Text"}</span>
            </button>
          </div>
        )}
      </div>

      {ocrText && (
        isCompact ? (
          <div className="clip-ocr-snippet">
            <HighlightedText text={ocrText} query={query} />
          </div>
        ) : (
          <div className="clip-ocr-card-container">
            <div className="clip-ocr-card-text">
              <HighlightedText text={ocrText} query={query} />
            </div>
          </div>
        )
      )}
    </div>
  );
};
