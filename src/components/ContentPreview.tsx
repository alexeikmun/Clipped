import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useMemo } from 'react';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';
// @ts-ignore
import createElement from 'react-syntax-highlighter/dist/esm/create-element';
import { HighlightedText, escapeRegExp } from './HighlightedText';

interface ContentPreviewProps {
  text: string;
  query: string;
}

const detectType = (text: string): 'json' | 'shell' | 'code' | 'text' => {
  const trimmed = text.trim();
  
  // JSON Detection
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
    try {
      JSON.parse(trimmed);
      return 'json';
    } catch (e) {
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

  // Code Detection (Heuristic)
  // Check for common programming keywords
  const codeKeywords = [
    'function', 'const', 'let', 'var', 'import', 'export', 'class', 'interface', 
    'return', 'if', 'else', 'for', 'while', 'switch', 'case', 'break', 'continue',
    'try', 'catch', 'finally', 'throw', 'new', 'this', 'super', 'extends', 'implements',
    'public', 'private', 'protected', 'static', 'void', 'null', 'true', 'false',
    'def', 'class', 'import', 'from', 'as', 'async', 'await', 'package', 'namespace',
    'using', 'include', '#include', '#define'
  ];
  
  // Simple tokenization by splitting on whitespace and non-alphanumeric
  const words = trimmed.split(/[\s(){}[\];.,<>:"'+=/-]+/);
  const keywordCount = words.filter(w => codeKeywords.includes(w)).length;
  
  // Check for structural indicators
  const hasBraces = trimmed.includes('{') && trimmed.includes('}');
  const hasSemicolons = trimmed.includes(';');
  const hasArrows = trimmed.includes('=>') || trimmed.includes('->');
  const hasParens = trimmed.includes('(') && trimmed.includes(')');
  
  // Heuristic: If we have keywords and structure, or just a lot of keywords
  if (keywordCount > 1 || (keywordCount > 0 && (hasBraces || hasSemicolons || hasArrows || hasParens))) {
    return 'code';
  }
  
  // Special case for simple function calls or assignments that might not match above
  if (/^[a-zA-Z_$][a-zA-Z0-9_$]*\s*\(.*\)\s*;?$/.test(trimmed)) return 'code'; // func()
  if (/^(const|let|var)\s+[a-zA-Z_$][a-zA-Z0-9_$]*\s*=/.test(trimmed)) return 'code'; // const x = ...

  return 'text';
};

export const ContentPreview = ({ text, query }: ContentPreviewProps) => {
  const type = detectType(text);
  
  // Custom style to remove default background and padding so it fits our list item
  const customStyle = {
    margin: 0,
    padding: 0,
    background: 'transparent',
    fontSize: 'inherit',
    lineHeight: 'inherit',
  };

  const terms = useMemo(() => query.split(/\s+/).filter(t => t.length > 0), [query]);
  const pattern = useMemo(() => {
     if (terms.length === 0) return null;
     const sortedTerms = [...terms].sort((a, b) => b.length - a.length);
     return new RegExp(`(${sortedTerms.map(escapeRegExp).join('|')})`, 'gi');
  }, [terms]);

  const renderer = ({ rows, stylesheet, useInlineStyles }: any) => {
    return rows.map((node: any, i: number) => {
      // Deep clone node to avoid mutating original if reused (though likely not)
      // Actually we need to traverse the node tree and split text nodes that match the query
      
      const highlightNode = (n: any): any => {
        if (n.type === 'text' && pattern) {
          const parts = n.value.split(pattern);
          if (parts.length === 1) return n;

          // If matched, we need to return an array of nodes, but we are inside map/recursion that expects one node object?
          // Wait, 'children' is an array of nodes. 'rows' is an array of nodes.
          // If I return an array here, the parent need to flatten it.
          // But I can't change the structure easily if I am just mapping children.
          // I should probably do a flatMap on children.
          
          // However, here we are modifying the 'n' object.
          // We can't turn one text node into multiple nodes *in place* without changing the parent's children array.
          
          // So I can't use simple recursion that returns 'n'.
          // I need a traversal that returns an array of nodes.
          return parts.map((part: string) => {
            const isMatch = terms.some(term => term.toLowerCase() === part.toLowerCase());
            if (isMatch) {
              return {
                type: 'element',
                tagName: 'span',
                properties: { className: ['highlight', 'exact'] },
                children: [{ type: 'text', value: part }]
              };
            }
            return { type: 'text', value: part };
          });
        }
        
        if (n.children) {
          n.children = n.children.flatMap(highlightNode);
        }
        return n;
      };

      // Since rows are top level nodes, we can flatMap them if we want, but renderer expects one element per row usually?
      // Actually, rows.map returns an array of React elements.
      // But 'createElement' takes a node.
      // I should process the node *before* passing to createElement.
      
      // We need to clone the node first to avoid mutation issues
      const nodeClone = JSON.parse(JSON.stringify(node));
      
      // If the top level node is a text node (unlikely for rows, usually element), handle it.
      // But usually rows are elements (span or element).
      // We need to handle the case where highlightNode returns an array.
      
      // If nodeClone is element, we process its children.
      if (nodeClone.children) {
        nodeClone.children = nodeClone.children.flatMap(highlightNode);
      } else if (nodeClone.type === 'text') {
         // If top level is text and it matches, we have a problem because we return array.
         // But usually rows are lines.
         const processed = highlightNode(nodeClone);
         if (Array.isArray(processed)) {
             // If it returns an array, we have to wrap it in a fragment or span?
             // createElement handles 'element' type.
             // We can wrap it in a span with no properties.
             return createElement({
                 node: {
                     type: 'element',
                    tagName: 'span',
                    properties: { className: [] },
                    children: processed
                 },
                 stylesheet,
                 useInlineStyles,
                 key: `code-segment-${i}`
             });
         }
         return createElement({ node: processed, stylesheet, useInlineStyles, key: `code-segment-${i}` });
      }

      return createElement({ node: nodeClone, stylesheet, useInlineStyles, key: `code-segment-${i}` });
    });
  };

  if (type === 'json') {
    return (
      <SyntaxHighlighter 
        language="json" 
        style={vscDarkPlus} 
        customStyle={customStyle}
        wrapLongLines={true}
        renderer={query ? renderer : undefined}
      >
        {text}
      </SyntaxHighlighter>
    );
  }

  if (type === 'shell') {
    return (
      <SyntaxHighlighter 
        language="bash" 
        style={vscDarkPlus} 
        customStyle={customStyle}
        wrapLongLines={true}
        renderer={query ? renderer : undefined}
      >
        {text}
      </SyntaxHighlighter>
    );
  }

  if (type === 'code') {
    return (
      <SyntaxHighlighter 
        language="typescript" // Good default for C-like languages
        style={vscDarkPlus} 
        customStyle={customStyle}
        wrapLongLines={true}
        renderer={query ? renderer : undefined}
      >
        {text}
      </SyntaxHighlighter>
    );
  }

  return <HighlightedText text={text} query={query} />;
};
