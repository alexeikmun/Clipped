import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { copyIconSvg, checkmarkIconSvg, imageFallbackIconSvg } from './icons';
import { 
  escapeHtml, 
  detectType, 
  tokenizeJson, 
  tokenizeShell, 
  tokenizeCode 
} from './tokenizer';

export { escapeHtml, detectType };

export interface ClipItem {
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

export function escapeRegExp(string: string): string {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function highlightText(text: string, query: string): string {
  const safeText = escapeHtml(text);
  const trimmed = query.trim();
  if (!trimmed) return safeText;

  const terms = trimmed.split(/\s+/).filter(t => t.length > 0);
  if (terms.length === 0) return safeText;

  const sortedTerms = [...terms].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`(${sortedTerms.map(escapeRegExp).join('|')})`, 'gi');

  return safeText.replace(pattern, (match) => {
    return `<span class="highlight exact">${match}</span>`;
  });
}

export function renderTextPreview(text: string, query: string): string {
  const type = detectType(text);

  let highlightedHtml: string;
  if (type === 'json') {
    highlightedHtml = tokenizeJson(text);
  } else if (type === 'shell') {
    highlightedHtml = tokenizeShell(text);
  } else if (type === 'code') {
    highlightedHtml = tokenizeCode(text);
  } else {
    highlightedHtml = escapeHtml(text);
  }

  return injectQueryHighlight(highlightedHtml, query);
}

// Injects search highlights into pre-highlighted HTML without breaking HTML tags
function injectQueryHighlight(html: string, query: string): string {
  const trimmed = query.trim();
  if (!trimmed) return html;

  const terms = trimmed.split(/\s+/).filter(t => t.length > 0);
  if (terms.length === 0) return html;

  const sortedTerms = [...terms].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`(${sortedTerms.map(escapeRegExp).join('|')})`, 'gi');

  // Split by HTML tags and only highlight text outside tags
  const parts = html.split(/(<[^>]+>)/g);
  for (let i = 0; i < parts.length; i++) {
    if (!parts[i].startsWith('<')) {
      parts[i] = parts[i].replace(pattern, (match) => `<span class="highlight exact">${match}</span>`);
    }
  }
  return parts.join('');
}

export function renderImagePreview(item: ClipItem, query: string, isCompact: boolean): string {
  const dimensionText = item.image_width && item.image_height 
    ? `${item.image_width} × ${item.image_height}` 
    : 'Image';

  let src = '';
  if (item.image_path) {
    try {
      src = ('__TAURI_INTERNALS__' in window) 
        ? convertFileSrc(item.image_path) 
        : item.image_path;
    } catch {
      src = '';
    }
  }

  const ocrHtml = item.ocr_text ? highlightText(item.ocr_text, query) : '';

  if (!src) {
    return `<div class="clip-image-fallback ${isCompact ? 'compact' : ''}">${imageFallbackIconSvg()}<div class="clip-image-details ${isCompact ? 'compact' : ''}"><div class="clip-image-meta"><span class="clip-image-badge">${dimensionText}</span></div>${item.ocr_text ? `<div class="clip-ocr-snippet">${ocrHtml}</div>` : ''}</div></div>`;
  }

  const ocrSection = item.ocr_text ? (
    isCompact
      ? `<div class="clip-ocr-snippet">${ocrHtml}</div>`
      : `<div class="clip-ocr-card-container"><div class="clip-ocr-card-text">${ocrHtml}</div></div>`
  ) : '';

  const ocrCopyAction = item.ocr_text ? `
    <div class="clip-ocr-actions">
      <span class="clip-ocr-badge">OCR</span>
      <button 
        type="button" 
        class="clip-ocr-copy-btn" 
        data-action="copy-ocr" 
        data-text="${escapeHtml(item.ocr_text)}" 
        title="Copy extracted text to clipboard"
      >
        ${copyIconSvg()}
        <span>Copy Text</span>
      </button>
    </div>
  ` : '';

  const ambientBlur = !isCompact
    ? `<div class="preview-image-ambient-blur" style="background-image: url('${src.replace(/'/g, "\\'")}');" aria-hidden="true"></div>`
    : '';

  return `<div class="clip-image-preview ${isCompact ? 'compact' : ''}"><div class="preview-image-wrapper ${isCompact ? 'compact' : ''}">${ambientBlur}<img src="${src}" alt="${escapeHtml(item.text || 'Clipboard Image')}" class="preview-image ${isCompact ? 'compact' : ''}" loading="lazy" onerror="this.parentElement.parentElement.classList.add('error')" /></div><div class="clip-image-details ${isCompact ? 'compact' : ''}"><div class="clip-image-meta"><span class="clip-image-badge">${dimensionText}</span>${ocrCopyAction}</div>${ocrSection}</div></div>`;
}

export async function copyOcrText(buttonEl: HTMLButtonElement, text: string): Promise<void> {
  try {
    if ('__TAURI_INTERNALS__' in window) {
      await invoke('copy_text', { text });
    } else if (navigator.clipboard) {
      await navigator.clipboard.writeText(text);
    }

    buttonEl.classList.add('copied');
    buttonEl.innerHTML = `${checkmarkIconSvg()}<span>Copied</span>`;
    setTimeout(() => {
      buttonEl.classList.remove('copied');
      buttonEl.innerHTML = `${copyIconSvg()}<span>Copy Text</span>`;
    }, 1500);
  } catch (err) {
    console.error('Failed to copy text:', err);
  }
}
