'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ChevronDown, ChevronUp, Copy, FileText, Plus, RotateCcw, Trash2, X } from 'lucide-react';

// Section field names are snake_case because Tauri's camelCase conversion only
// applies to command arguments, not to nested struct fields.
type TemplateFormat = 'paragraph' | 'list' | 'string';

interface TemplateSection {
  title: string;
  instruction: string;
  format: TemplateFormat;
  item_format?: string | null;
}

interface Template {
  name: string;
  description: string;
  sections: TemplateSection[];
}

interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
}

/** `id: null` means "create", anything else overwrites that template. */
interface Draft {
  id: string | null;
  template: Template;
}

const EMPTY_SECTION: TemplateSection = { title: '', instruction: '', format: 'paragraph' };

function isComplete(template: Template): boolean {
  return (
    template.name.trim() !== '' &&
    template.description.trim() !== '' &&
    template.sections.length > 0 &&
    template.sections.every((s) => s.title.trim() !== '' && s.instruction.trim() !== '')
  );
}

export function SummaryTemplateSettings() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setTemplates(await invoke<TemplateInfo[]>('api_list_templates'));
    } catch (error) {
      console.error('Failed to list templates:', error);
      toast.error('Failed to load summary templates');
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const load = async (id: string) => invoke<Template>('api_get_template_details', { templateId: id });

  const startEdit = async (id: string) => {
    try {
      setDraft({ id, template: await load(id) });
    } catch (error) {
      console.error('Failed to load template:', error);
      toast.error('Failed to open template');
    }
  };

  // Both "New" and "Duplicate" seed from an existing template — a blank form
  // gives no hint what an instruction is supposed to sound like.
  const startCreate = async (from: string, name: string) => {
    try {
      const template = await load(from);
      setDraft({ id: null, template: { ...template, name } });
    } catch (error) {
      console.error('Failed to seed template:', error);
      toast.error('Failed to create template');
    }
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      await invoke('api_save_template', { templateId: draft.id, template: draft.template });
      toast.success(`Saved "${draft.template.name}"`);
      setDraft(null);
      await refresh();
    } catch (error) {
      console.error('Failed to save template:', error);
      toast.error(typeof error === 'string' ? error : 'Failed to save template');
    } finally {
      setBusy(false);
    }
  };

  const remove = async ({ id, name, builtin }: TemplateInfo) => {
    // ponytail: native confirm — a bespoke dialog for two destructive buttons isn't worth it.
    const message = builtin
      ? `Reset "${name}" to the version shipped with the app? Your changes to it will be lost.`
      : `Delete "${name}"? This cannot be undone.`;
    if (!window.confirm(message)) return;

    setBusy(true);
    try {
      await invoke('api_delete_template', { templateId: id });
      toast.success(builtin ? `Reset "${name}"` : `Deleted "${name}"`);
      if (draft?.id === id) setDraft(null);
      await refresh();
    } catch (error) {
      console.error('Failed to delete template:', error);
      toast.error(typeof error === 'string' ? error : 'Failed to delete template');
    } finally {
      setBusy(false);
    }
  };

  const patch = (changes: Partial<Template>) =>
    setDraft((prev) => (prev ? { ...prev, template: { ...prev.template, ...changes } } : prev));

  const patchSection = (index: number, changes: Partial<TemplateSection>) =>
    patch({
      sections: draft!.template.sections.map((s, i) => (i === index ? { ...s, ...changes } : s)),
    });

  const moveSection = (index: number, offset: number) => {
    const sections = [...draft!.template.sections];
    const target = index + offset;
    if (target < 0 || target >= sections.length) return;
    [sections[index], sections[target]] = [sections[target], sections[index]];
    patch({ sections });
  };

  const canSave = draft !== null && isComplete(draft.template);

  return (
    <div className="bg-elevated rounded-lg border border-line p-6 shadow-sm">
      <div className="flex items-start justify-between gap-4 mb-2">
        <div className="flex items-center gap-2">
          <FileText size={18} className="text-ink-muted" />
          <h3 className="text-lg font-semibold text-ink">Summary Templates</h3>
        </div>
        <button
          type="button"
          onClick={() => startCreate('standard_meeting', 'New Template')}
          disabled={busy || draft !== null}
          className="inline-flex items-center gap-1.5 rounded-md bg-brand px-3 py-1.5 text-sm font-medium text-brand-ink hover:bg-brand-hover disabled:opacity-50"
        >
          <Plus size={14} />
          New template
        </button>
      </div>
      <p className="text-sm text-ink-muted mb-4">
        Each template defines the sections of a generated summary and tells the model what to put in
        them. Templates that ship with the app can be edited and reset again.
      </p>

      <ul className="divide-y divide-line">
        {templates.map((info) => (
          <li key={info.id} className="py-3">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <p className="text-sm font-medium text-ink">{info.name}</p>
                <p className="text-xs text-ink-muted">{info.description}</p>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  onClick={() => (draft?.id === info.id ? setDraft(null) : startEdit(info.id))}
                  disabled={busy}
                  className="rounded-md px-2 py-1 text-sm text-ink hover:bg-sunken disabled:opacity-50"
                >
                  {draft?.id === info.id ? 'Close' : 'Edit'}
                </button>
                <button
                  type="button"
                  aria-label={`Duplicate ${info.name}`}
                  title="Duplicate"
                  onClick={() => startCreate(info.id, `${info.name} (copy)`)}
                  disabled={busy || draft !== null}
                  className="rounded-md p-1.5 text-ink-muted hover:bg-sunken hover:text-ink disabled:opacity-50"
                >
                  <Copy size={15} />
                </button>
                <button
                  type="button"
                  aria-label={info.builtin ? `Reset ${info.name}` : `Delete ${info.name}`}
                  title={info.builtin ? 'Reset to default' : 'Delete'}
                  onClick={() => remove(info)}
                  disabled={busy}
                  className="rounded-md p-1.5 text-ink-muted hover:bg-danger-soft hover:text-danger-ink disabled:opacity-50"
                >
                  {info.builtin ? <RotateCcw size={15} /> : <Trash2 size={15} />}
                </button>
              </div>
            </div>

            {draft?.id === info.id && renderEditor()}
          </li>
        ))}
      </ul>

      {draft?.id === null && (
        <div className="mt-4 border-t border-line pt-4">
          <p className="mb-2 text-sm font-medium text-ink">New template</p>
          {renderEditor()}
        </div>
      )}
    </div>
  );

  // Inlined so it closes over `draft` and the patch helpers without threading a
  // dozen props through a second component. Called as a function, not rendered as
  // <Editor /> — a component declared here is a new type every render, which would
  // remount the inputs and drop focus on every keystroke.
  function renderEditor() {
    if (!draft) return null;
    const { template } = draft;

    return (
      <div className="mt-3 space-y-4 rounded-md bg-sunken p-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-ink-muted">Name</span>
            <input
              value={template.name}
              onChange={(e) => patch({ name: e.target.value })}
              className="w-full rounded-md border border-line bg-elevated px-3 py-2 text-sm text-ink focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </label>
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-ink-muted">Description</span>
            <input
              value={template.description}
              onChange={(e) => patch({ description: e.target.value })}
              className="w-full rounded-md border border-line bg-elevated px-3 py-2 text-sm text-ink focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </label>
        </div>

        {template.sections.map((section, index) => (
          <div key={index} className="rounded-md border border-line bg-elevated p-3">
            <div className="mb-2 flex items-center gap-2">
              <input
                value={section.title}
                onChange={(e) => patchSection(index, { title: e.target.value })}
                placeholder="Section title"
                className="flex-1 rounded-md border border-line bg-elevated px-3 py-1.5 text-sm font-medium text-ink focus:outline-none focus:ring-1 focus:ring-ring"
              />
              <select
                value={section.format}
                onChange={(e) =>
                  patchSection(index, {
                    format: e.target.value as TemplateFormat,
                    // item_format only means anything for lists
                    item_format: e.target.value === 'list' ? section.item_format ?? null : null,
                  })
                }
                className="rounded-md border border-line bg-elevated px-2 py-1.5 text-sm text-ink focus:outline-none focus:ring-1 focus:ring-ring"
              >
                <option value="paragraph">Paragraph</option>
                <option value="list">List</option>
                <option value="string">Single line</option>
              </select>
              <button
                type="button"
                aria-label="Move section up"
                onClick={() => moveSection(index, -1)}
                disabled={index === 0}
                className="rounded-md p-1 text-ink-muted hover:bg-sunken hover:text-ink disabled:opacity-30"
              >
                <ChevronUp size={15} />
              </button>
              <button
                type="button"
                aria-label="Move section down"
                onClick={() => moveSection(index, 1)}
                disabled={index === template.sections.length - 1}
                className="rounded-md p-1 text-ink-muted hover:bg-sunken hover:text-ink disabled:opacity-30"
              >
                <ChevronDown size={15} />
              </button>
              <button
                type="button"
                aria-label="Remove section"
                onClick={() =>
                  patch({ sections: template.sections.filter((_, i) => i !== index) })
                }
                className="rounded-md p-1 text-ink-muted hover:bg-danger-soft hover:text-danger-ink"
              >
                <X size={15} />
              </button>
            </div>

            <textarea
              value={section.instruction}
              onChange={(e) => patchSection(index, { instruction: e.target.value })}
              placeholder="What should the model extract for this section?"
              rows={2}
              className="w-full rounded-md border border-line bg-elevated px-3 py-2 text-sm text-ink focus:outline-none focus:ring-1 focus:ring-ring"
            />

            {section.format === 'list' && (
              <label className="mt-2 block">
                <span className="mb-1 block text-xs font-medium text-ink-muted">
                  Item format (optional) — e.g. a markdown table header
                </span>
                <input
                  value={section.item_format ?? ''}
                  onChange={(e) =>
                    patchSection(index, { item_format: e.target.value || null })
                  }
                  className="w-full rounded-md border border-line bg-elevated px-3 py-2 font-mono text-xs text-ink focus:outline-none focus:ring-1 focus:ring-ring"
                />
              </label>
            )}
          </div>
        ))}

        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={() => patch({ sections: [...template.sections, { ...EMPTY_SECTION }] })}
            className="inline-flex items-center gap-1.5 rounded-md border border-line px-3 py-1.5 text-sm text-ink hover:bg-elevated"
          >
            <Plus size={14} />
            Add section
          </button>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setDraft(null)}
              className="rounded-md px-3 py-1.5 text-sm text-ink hover:bg-elevated"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={save}
              disabled={!canSave || busy}
              title={canSave ? undefined : 'Every section needs a title and an instruction'}
              className="rounded-md bg-brand px-3 py-1.5 text-sm font-medium text-brand-ink hover:bg-brand-hover disabled:opacity-50"
            >
              Save
            </button>
          </div>
        </div>
      </div>
    );
  }
}
