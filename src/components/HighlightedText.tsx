export const escapeRegExp = (string: string) => {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
};

export const HighlightedText = ({ text, query }: { text: string; query: string }) => {
  if (!query) {
    return <span>{text}</span>;
  }

  const terms = query.split(/\s+/).filter(t => t.length > 0);
  if (terms.length === 0) {
    return <span>{text}</span>;
  }

  // Sort by length descending to ensure longest matches are prioritized in the regex
  const sortedTerms = [...terms].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`(${sortedTerms.map(escapeRegExp).join('|')})`, 'gi');
  
  const parts = text.split(pattern);
  return (
    <>
      {parts.map((part, i) => {
        const isMatch = terms.some(term => term.toLowerCase() === part.toLowerCase());
        return isMatch ? (
          <span key={i} className="highlight exact">{part}</span>
        ) : (
          <span key={i}>{part}</span>
        );
      })}
    </>
  );
};
