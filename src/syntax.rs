#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Property,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxToken {
    pub start_u16: u32,
    pub length_u16: u32,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Json,
    Shell,
    Code,
    Text,
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "const", "var", "function", "class", "interface", "type", "struct",
    "enum", "impl", "pub", "use", "mod", "import", "export", "from", "as", "return",
    "if", "else", "match", "switch", "case", "for", "while", "loop", "break", "continue",
    "yield", "async", "await", "try", "catch", "finally", "throw", "new", "this", "self",
    "super", "extends", "implements", "static", "void", "null", "undefined", "true", "false",
    "def", "lambda", "with", "pass", "package", "select", "from", "where", "insert", "update",
    "delete", "create", "table", "in", "is", "not", "and", "or", "do", "val", "trait",
    "override", "namespace", "nil", "None", "Some", "Ok", "Err",
];

const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "char", "str", "String", "Option", "Result",
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64", "Vec", "HashMap", "HashSet", "Box", "Rc", "Arc",
    "int", "float", "double", "boolean", "any", "unknown", "never",
];

const SHELL_COMMANDS: &[&str] = &[
    "sudo", "git", "npm", "pnpm", "yarn", "cargo", "docker", "cd", "ls", "echo",
    "cat", "grep", "ssh", "curl", "wget", "rm", "mv", "cp", "mkdir", "touch",
    "ps", "kill", "top", "htop", "chmod", "chown", "tar", "zip", "unzip", "brew",
    "apt", "apt-get", "yum", "dnf", "pacman", "systemctl", "journalctl", "find",
];

pub fn detect_language(text: &str) -> Language {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Language::Text;
    }

    // 1. JSON check (cap size to 32KB to avoid heavy AST allocation in paint loop)
    if trimmed.len() <= 32768
        && ((trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']')))
    {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return Language::Json;
        }
    }

    // 2. Shell command check
    for cmd in SHELL_COMMANDS {
        if trimmed.starts_with(&format!("{} ", cmd))
            || trimmed == *cmd
            || trimmed.starts_with(&format!("$ {} ", cmd))
            || trimmed.starts_with(&format!("$ {}", cmd))
        {
            return Language::Shell;
        }
    }
    if trimmed.starts_with("$ ") || trimmed.starts_with("# ") {
        return Language::Shell;
    }

    // 3. Code heuristic
    let mut keyword_count = 0;
    for word in trimmed.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if KEYWORDS.contains(&word) {
            keyword_count += 1;
        }
    }
    let has_structure = trimmed.contains('{') || trimmed.contains(';') || trimmed.contains("=>") || trimmed.contains("->");
    if keyword_count >= 2 || (keyword_count >= 1 && has_structure) {
        return Language::Code;
    }

    Language::Text
}

pub fn tokenize(text: &str) -> Vec<SyntaxToken> {
    // Only tokenize up to first 4,000 characters (card fits ~2,000 max)
    // Ensures sub-millisecond execution even on massive clipboard clips
    let text_slice = if text.len() > 4000 {
        let mut end = 4000;
        while !text.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    let chars: Vec<char> = text_slice.chars().collect();
    let len = chars.len();
    if len == 0 {
        return Vec::new();
    }

    // Build UTF-16 offset lookup: u16_offsets[char_idx]
    let mut u16_offsets = Vec::with_capacity(len + 1);
    let mut cur_u16 = 0u32;
    for &c in &chars {
        u16_offsets.push(cur_u16);
        cur_u16 += c.len_utf16() as u32;
    }
    u16_offsets.push(cur_u16);

    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        // 1. Line comment: // or #
        if (c == '/' && i + 1 < len && chars[i + 1] == '/') || (c == '#' && (i == 0 || chars[i - 1] == '\n' || chars[i - 1] == ' ')) {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];
            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 2. Block comment: /* ... */
        if c == '/' && i + 1 < len && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            } else {
                i = len;
            }
            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];
            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind: TokenKind::Comment,
            });
            continue;
        }

        // 3. Strings: "...", '...', `...`
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            let start = i;
            i += 1;
            while i < len {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else if chars[i] == quote {
                    i += 1;
                    break;
                } else if chars[i] == '\n' && quote != '`' {
                    break;
                } else {
                    i += 1;
                }
            }

            // Check if string is a JSON key or object property (followed by optional whitespace and ':')
            let mut is_key = false;
            let mut k = i;
            while k < len && (chars[k] == ' ' || chars[k] == '\t') {
                k += 1;
            }
            if k < len && chars[k] == ':' {
                is_key = true;
            }

            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];
            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind: if is_key { TokenKind::Property } else { TokenKind::String },
            });
            continue;
        }

        // 4. Shell flags: -rf, --release, -m
        if c == '-' && i + 1 < len && (chars[i + 1].is_alphabetic() || chars[i + 1] == '-') {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                i += 1;
            }
            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];
            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind: TokenKind::Type,
            });
            continue;
        }

        // 5. Numbers: 123, 0x1f, 3.14
        if c.is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let start = i;
            let mut is_hex = false;
            if c == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                is_hex = true;
                i += 2;
            }
            while i < len {
                let ch = chars[i];
                if is_hex && ch.is_ascii_hexdigit() {
                    i += 1;
                } else if !is_hex && (ch.is_ascii_digit() || ch == '.' || ch == '_') {
                    i += 1;
                } else {
                    break;
                }
            }
            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];
            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind: TokenKind::Number,
            });
            continue;
        }

        // 6. Words: identifiers, keywords, types, function calls
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            i += 1;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                i += 1;
            }

            let word: String = chars[start..i].iter().collect();
            let u16_start = u16_offsets[start];
            let u16_end = u16_offsets[i];

            // Lookahead for function call: foo( or foo!
            let mut is_call = false;
            let mut k = i;
            while k < len && (chars[k] == ' ' || chars[k] == '\t') {
                k += 1;
            }
            if k < len && (chars[k] == '(' || (chars[k] == '!' && k + 1 < len && chars[k + 1] != '=')) {
                is_call = true;
            }

            // Lookbehind for property access: .foo
            let is_prop = start > 0 && chars[start - 1] == '.';

            let kind = if KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if SHELL_COMMANDS.contains(&word.as_str()) {
                TokenKind::Function
            } else if PRIMITIVE_TYPES.contains(&word.as_str()) {
                TokenKind::Type
            } else if is_call {
                TokenKind::Function
            } else if is_prop {
                TokenKind::Property
            } else if word.chars().next().map_or(false, |ch| ch.is_uppercase()) {
                TokenKind::Type
            } else {
                continue;
            };

            tokens.push(SyntaxToken {
                start_u16: u16_start,
                length_u16: u16_end - u16_start,
                kind,
            });
            continue;
        }

        i += 1;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("{\"name\": \"clipped\"}"), Language::Json);
        assert_eq!(detect_language("cargo build --release"), Language::Shell);
        assert_eq!(detect_language("fn main() {\n    let x = 42;\n}"), Language::Code);
        assert_eq!(detect_language("Hello world clipboard text"), Language::Text);
    }

    #[test]
    fn test_tokenize_json() {
        let json = "{\n  \"name\": \"clipped\",\n  \"version\": 2\n}";
        let tokens = tokenize(json);
        assert!(!tokens.is_empty());
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Property));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Number));
    }

    #[test]
    fn test_tokenize_code() {
        let code = "fn add(a: i32, b: i32) -> i32 {\n    // Sum\n    a + b\n}";
        let tokens = tokenize(code);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Type));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Function));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Comment));
    }

    #[test]
    fn test_tokenize_dollar_and_shell() {
        // Prevents regression of infinite loop on dollar sign
        let shell = "$ npm run dev\n$var = 42;\n$\n${TEST}";
        let tokens = tokenize(shell);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_tokenize_large_clip_safety() {
        let large = "let x = 1;\n".repeat(10_000);
        let tokens = tokenize(&large);
        assert!(!tokens.is_empty());
    }
}
