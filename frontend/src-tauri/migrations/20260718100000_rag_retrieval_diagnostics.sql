-- Preserve local retrieval diagnostics with every knowledge-base answer.  This lets
-- restored chats explain whether an answer was absent because the index was incomplete,
-- semantic search was unavailable, or no grounded evidence passed the relevance gate.
ALTER TABLE chat_messages
    ADD COLUMN retrieval_diagnostics TEXT NOT NULL DEFAULT '{}';
