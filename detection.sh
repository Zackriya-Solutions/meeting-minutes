#!/usr/bin/env bash

OUTPUT="commit_context.md"

echo "# Commit Context" > "$OUTPUT"
echo "" >> "$OUTPUT"

git diff --name-only | while read file
do
    echo "## $file" >> "$OUTPUT"

    git diff --unified=0 -- "$file" \
    | grep '^@@' \
    | head -5 \
    | sed -E 's/.*@@ ?//' \
    | while read context
    do
        echo "- $context" >> "$OUTPUT"
    done

    echo "" >> "$OUTPUT"

done