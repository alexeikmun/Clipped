export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

export type ClipSnippetType = 'json' | 'shell' | 'code' | 'text';

export const detectType = (text: string): ClipSnippetType => {
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
    /^sudo\s/, /^npm\s/, /^npx\s/, /^git\s/, /^docker\s/, /^cargo\s/, /^pnpm\s/, /^yarn\s/, /^bun\s/,
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
    'def', 'async', 'await', 'package', 'namespace', 'using', 'include', '#include', '#define',
    'fn', 'pub', 'mut', 'impl', 'trait', 'struct', 'match'
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

// 1. JSON Tokenizer
const jsonRegex = /("(?:\\.|[^"\\])*"(?:\s*:)?)|(-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)|\b(true|false|null)\b|([{}[\],:])/g;

export function tokenizeJson(code: string): string {
  const rx = new RegExp(jsonRegex.source, 'g');
  let lastIndex = 0;
  let html = '';
  let m: RegExpExecArray | null;

  while ((m = rx.exec(code)) !== null) {
    if (m.index > lastIndex) {
      html += escapeHtml(code.slice(lastIndex, m.index));
    }

    const str = m[1];
    const num = m[2];
    const bool = m[3];
    const punc = m[4];

    if (str) {
      if (str.endsWith(':')) {
        const key = str.slice(0, -1).trimEnd();
        const colon = str.slice(key.length);
        html += `<span class="token property">${escapeHtml(key)}</span><span class="token punctuation">${escapeHtml(colon)}</span>`;
      } else {
        html += `<span class="token string">${escapeHtml(str)}</span>`;
      }
    } else if (num) {
      html += `<span class="token number">${escapeHtml(num)}</span>`;
    } else if (bool) {
      html += `<span class="token boolean">${escapeHtml(bool)}</span>`;
    } else if (punc) {
      html += `<span class="token punctuation">${escapeHtml(punc)}</span>`;
    }

    lastIndex = rx.lastIndex;
  }

  if (lastIndex < code.length) {
    html += escapeHtml(code.slice(lastIndex));
  }

  return html;
}

// 2. Shell Tokenizer
const shellRegex = /(#[^\n]*)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(--?[a-zA-Z0-9_-]+)|\b(sudo|npm|npx|pnpm|yarn|bun|git|docker|cargo|rustc|go|python|node|deno|cd|ls|echo|cat|grep|ssh|curl|wget|rm|mv|cp|mkdir|touch|ps|kill|top|htop|chmod|chown|tar|zip|unzip|brew|apt|apt-get|yum|dnf|pacman|systemctl|journalctl)\b|(\b\d+\b)|([|&;><$]+)/g;

export function tokenizeShell(code: string): string {
  const rx = new RegExp(shellRegex.source, 'g');
  let lastIndex = 0;
  let html = '';
  let m: RegExpExecArray | null;

  while ((m = rx.exec(code)) !== null) {
    if (m.index > lastIndex) {
      html += escapeHtml(code.slice(lastIndex, m.index));
    }

    const [_, comment, str, flag, cmd, num, op] = m;
    if (comment) {
      html += `<span class="token comment">${escapeHtml(comment)}</span>`;
    } else if (str) {
      html += `<span class="token string">${escapeHtml(str)}</span>`;
    } else if (flag) {
      html += `<span class="token variable">${escapeHtml(flag)}</span>`;
    } else if (cmd) {
      html += `<span class="token keyword">${escapeHtml(cmd)}</span>`;
    } else if (num) {
      html += `<span class="token number">${escapeHtml(num)}</span>`;
    } else if (op) {
      html += `<span class="token operator">${escapeHtml(op)}</span>`;
    }

    lastIndex = rx.lastIndex;
  }

  if (lastIndex < code.length) {
    html += escapeHtml(code.slice(lastIndex));
  }

  return html;
}

// 3. Code Tokenizer (JS, TS, Rust, Python, Go, C/C++, etc.)
const codeRegex = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\b(?:const|let|var|function|return|if|else|for|while|switch|case|break|continue|try|catch|finally|throw|new|this|super|extends|implements|public|private|protected|static|void|null|true|false|undefined|async|await|import|export|from|as|default|class|interface|type|enum|namespace|package|using|include|fn|pub|mut|impl|trait|struct|match|def|elif|lambda|self|echo|print)\b)|(\b[A-Z][a-zA-Z0-9_$]*\b)|(\b[a-zA-Z_$][a-zA-Z0-9_$]*(?=\s*\())|(\b(?:0x[0-9a-fA-F]+|\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)\b)|(=>|->|===|!==|==|!=|<=|>=|&&|\|\||[+\-*/%&|^!=<>?:])|([{}()[\],;.])/g;

export function tokenizeCode(code: string): string {
  const rx = new RegExp(codeRegex.source, 'g');
  let lastIndex = 0;
  let html = '';
  let m: RegExpExecArray | null;

  while ((m = rx.exec(code)) !== null) {
    if (m.index > lastIndex) {
      html += escapeHtml(code.slice(lastIndex, m.index));
    }

    const [_, comment, str, kw, typeName, fn, num, op, punc] = m;
    if (comment) {
      html += `<span class="token comment">${escapeHtml(comment)}</span>`;
    } else if (str) {
      html += `<span class="token string">${escapeHtml(str)}</span>`;
    } else if (kw) {
      html += `<span class="token keyword">${escapeHtml(kw)}</span>`;
    } else if (typeName) {
      html += `<span class="token class-name">${escapeHtml(typeName)}</span>`;
    } else if (fn) {
      html += `<span class="token function">${escapeHtml(fn)}</span>`;
    } else if (num) {
      html += `<span class="token number">${escapeHtml(num)}</span>`;
    } else if (op) {
      html += `<span class="token operator">${escapeHtml(op)}</span>`;
    } else if (punc) {
      html += `<span class="token punctuation">${escapeHtml(punc)}</span>`;
    }

    lastIndex = rx.lastIndex;
  }

  if (lastIndex < code.length) {
    html += escapeHtml(code.slice(lastIndex));
  }

  return html;
}
