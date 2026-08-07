# Meeting Summary Templates

This directory contains template definitions for meeting summary generation.

## Available Templates

### 1. `daily_standup.json`
Time-boxed daily updates template designed for engineering/product teams.

**Sections:**
- Date
- Attendees
- Yesterday (completed work)
- Today (planned work)
- Blockers
- Notes

### 2. `standard_meeting.json`
General-purpose meeting notes template focusing on key outcomes and actions.
Also the default when no template is selected.

**Sections:**
- Summary
- Key Decisions
- Action Items
- Discussion Highlights

### 3. `retrospective.json`
Sprint/project retrospective template.

## Template Structure

Each template JSON file follows this schema:

```json
{
  "name": "Template Name",
  "description": "Brief description of the template's purpose",
  "sections": [
    {
      "title": "Section Title",
      "instruction": "Instructions for the LLM on what to extract/include",
      "format": "paragraph|list|string",
      "item_format": "Optional: Markdown table format for list items"
    }
  ]
}
```

## Custom Templates

Templates are managed in the app under **Settings → Summary → Summary Templates**,
which writes JSON into the application data directory:

- **macOS**: `~/Library/Application Support/Conversationaly/templates/`
- **Windows**: `%APPDATA%\Conversationaly\templates\`
- **Linux**: `~/.config/Conversationaly/templates/`

Custom templates override the templates in this directory when the filename matches,
which is how editing a shipped template works — and deleting the custom copy is what
"Reset to default" does. Dropping JSON files in there by hand still works.

## Template Fields

### Root Level
- `name` (required): Display name for the template
- `description` (required): Brief explanation of the template's use case
- `sections` (required): Array of section definitions

### Section Object
- `title` (required): Section heading text
- `instruction` (required): LLM guidance for this section
- `format` (required): One of `"paragraph"`, `"list"`, or `"string"`
- `item_format` (optional): Markdown formatting hint for list items (e.g., table structure)
- `example_item_format` (optional): Alternative formatting hint

## Usage in Code

Templates are loaded using the `templates` module:

```rust
use crate::summary::templates;

// Get a specific template
let template = templates::get_template("daily_standup")?;

// List available templates (id, name, description)
let available = templates::list_templates();

// Create or overwrite a user template; None generates the id from the name
let id = templates::save_template(None, &template)?;

// Remove a user template, or reset a shipped one to its bundled version
templates::delete_template("my_notes")?;
```
