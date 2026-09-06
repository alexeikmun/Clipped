import { useState, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

interface ImagePreviewProps {
  imagePath?: string;
  width?: number;
  height?: number;
  text?: string;
  isCompact?: boolean;
}

export const ImagePreview = ({
  imagePath,
  width,
  height,
  text,
  isCompact = false,
}: ImagePreviewProps) => {
  const [src, setSrc] = useState<string>("");
  const [hasError, setHasError] = useState(false);

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
      </div>
    </div>
  );
};
