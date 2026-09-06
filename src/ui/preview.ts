import Prism from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-typescript';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { copyIconSvg, checkmarkIconSvg, imageFallbackIconSvg } from './icons';

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

export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
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

export const detectType = (text: string): 'json' | 'shell' | 'code' | 'text' => {
  const trimmed = text.trim();

  // JSON Detection
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
    try {
      JSON.parse(trimmed);
      return 'json';
    } catch {
      // Not valid JSON
    }
  }

  // Shell Command Detection
  const shellPatterns = [
    /^sudo\s/, /^npm\s/, /^git\s/, /^docker\s/, /^cargo\s/, /^pnpm\s/, /^yarn\s/,
    /^cd\s/, /^ls\s/, /^echo\s/, /^cat\s/, /^grep\s/, /^ssh\s/, /^\$\s/,
    /^curl\s/, /^wget\s/, /^rm\s/, /^mv\s/, /^cp\s/, /^mkdir\s/, /^touch\s/,
    /^ps\s/, /^kill\s/, /^top\s/, /^htop\s/, /^chmod\s/, /^chown\s/, /^tar\s/,
    /^zip\s/, /^unzip\s/, /^brew\s/, /^apt\s/, /^apt-get\s/, /^yum\s/, /^dnf\s/,
    /^pacman\s/, /^systemctl\s/, /^journalctl\s/
  ];

  if (shellPatterns.some(p => p.test(trimmed))) {
    return 'shell';
  }

  // Code Detection
  const codeKeywords = [
    'function', 'const', 'let', 'var', 'import', 'export', 'class', 'interface', 
    'return', 'if', 'else', 'for', 'while', 'switch', 'case', 'break', 'continue',
    'try', 'catch', 'finally', 'throw', 'new', 'this', 'super', 'extends', 'implements',
    'public', 'private', 'protected', 'static', 'void', 'null', 'true', 'false',
    'def', 'async', 'await', 'package', 'namespace', 'using', 'include', '#include', '#define'
  ];

  const words = trimmed.split(/[\s(){}[\];.,<>:"'+=/-]+/);
  const keywordCount = words.filter(w => codeKeywords.includes(w)).length;

  const hasBraces = trimmed.includes('{') && trimmed.includes('}');
  const hasSemicolons = trimmed.includes(';');
  const hasArrows = trimmed.includes('=>') || trimmed.includes('->');
  const hasParens = trimmed.includes('(') && trimmed.includes(')');

  if (keywordCount > 1 || (keywordCount > 0 && (hasBraces || hasSemicolons || hasArrows || hasParens))) {
    return 'code';
  }

  if (/^[a-zA-Z_$][a-zA-Z0-9_$]*\s*\(.*\)\s*;?$/.test(trimmed)) return 'code';
  if (/^(const|let|var)\s+[a-zA-Z_$][a-zA-Z0-9_$]*\s*=/.test(trimmed)) return 'code';

  return 'text';
};

export function renderTextPreview(text: string, query: string): string {
  const type = detectType(text);

  if (type === 'json' && Prism.languages.json) {
    try {
      const highlighted = Prism.highlight(text, Prism.languages.json, 'json');
      return injectQueryHighlight(highlighted, query);
    } catch {
      return highlightText(text, query);
    }
  }

  if (type === 'shell' && Prism.languages.bash) {
    try {
      const highlighted = Prism.highlight(text, Prism.languages.bash, 'bash');
      return injectQueryHighlight(highlighted, query);
    } catch {
      return highlightText(text, query);
    }
  }

  if (type === 'code' && Prism.languages.typescript) {
    try {
      const highlighted = Prism.highlight(text, Prism.languages.typescript, 'typescript');
      return injectQueryHighlight(highlighted, query);
    } catch {
      return highlightText(text, query);
    }
  }

  return highlightText(text, query);
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
