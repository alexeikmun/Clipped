export const escapeRegExp = (string: string) => {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
};

export const HighlightedText = ({ text, query }: { text: string; query: string }) => {
  if (!query) {
    return <span>{text}</span>;
  }

  const parts = text.split(new RegExp(`(${escapeRegExp(query)})`, 'gi'));
  return (
    <>
      {parts.map((part, i) => 
        part.toLowerCase() === query.toLowerCase() ? (
          <span key={i} className="highlight exact">{part}</span>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
};
