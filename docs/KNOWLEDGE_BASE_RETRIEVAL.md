# Knowledge-base retrieval

Memento answers archive questions only from transcript evidence. Retrieval is local;
only the selected evidence fragments and the question reach the configured answer model.

## Query path

1. Normalize case, `ё/е`, punctuation, and remove stop words from the lexical branch.
2. Expand common Latin/Cyrillic and ASR spellings. The built-in Memento/Meetily aliases
   cover the product-history corpus.
3. Add aliases from **confirmed** terminology memory. Pending or model-generated terms
   never influence retrieval.
4. Embed the corrected semantic form with the selected local embedder.
5. Retrieve independently through canonical FTS, expanded/prefix FTS, bounded fuzzy
   matching, meeting titles, and cosine vector search. Edit-distance scoring is capped
   at 512 candidates; lexical and semantic top-K candidates are always retained.
6. Fuse ranks with RRF, then calculate confidence from lexical coverage and cosine
   similarity. RRF rank itself is never treated as confidence.
7. Diversify the answer context across meetings (up to three fragments per meeting).
8. If chunking is still pending, search the original transcript rows as temporary units.
9. Generate a cross-meeting answer with source markers and reject unsupported answers.

## Failure states

The UI distinguishes:

- `no_index`: the selected scope contains no searchable transcripts;
- `index_incomplete`: only part of the scope has chunks (transcript fallback was also
  attempted);
- `no_relevant_evidence`: the index is ready, but evidence did not pass the relevance
  gate;
- `answer_not_found`: relevant evidence reached the answer model, but it determined
  that the fragments do not answer the question;
- `answer_ungrounded`: evidence was found, but the answer model failed the citation
  check twice and the answer was rejected;
- `ok`: grounded evidence was sent to the answer model.

Diagnostics are stored locally with assistant chat messages. They include branch hit
counts, semantic availability, index coverage, query rewriting, and the best evidence
score. This makes repeated “not found” reports diagnosable without logging transcript
content or questions externally.

## Regression contract

Tests cover:

- the typo-heavy `митили → мементо` product-history question;
- Russian inflections and ASR misspellings;
- confirmed terminology aliases such as `пайплайн → pipeline`;
- scope and privacy filters;
- unchunked transcript fallback;
- cross-branch rank fusion and confidence guards.

Any retrieval-model or threshold change should extend these fixtures and report recall,
false-positive rate, and citation validity before release.
