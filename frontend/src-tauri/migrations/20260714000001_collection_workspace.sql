-- Collections are read from the collection side in the workspace and in
-- collection-scoped RAG. The original composite primary key starts with
-- meeting_id, so add the reverse lookup used by both paths.
CREATE INDEX IF NOT EXISTS idx_meeting_collections_collection
    ON meeting_collections(collection_id, meeting_id);
