'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronDown, ChevronUp, FileText, Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';

type TemplateKind = 'builtin' | 'custom' | 'edited' | 'new';
type SectionFormat = 'paragraph' | 'list' | 'string';

interface TemplateSummary {
  id: string;
  name: string;
  description: string;
  kind: TemplateKind;
}

interface TemplateSection {
  title: string;
  instruction: string;
  format: SectionFormat;
  itemFormat: string;
}

interface EditorTemplate {
  id: string;
  kind: Exclude<TemplateKind, 'new'>;
  name: string;
  description: string;
  sections: Array<{
    title: string;
    instruction: string;
    format: string;
    itemFormat?: string | null;
  }>;
}

interface Draft {
  id: string | null;
  kind: TemplateKind;
  name: string;
  description: string;
  sections: TemplateSection[];
}

type Pending =
  | { type: 'delete' }
  | { type: 'reset' }
  | { type: 'discard'; next: { type: 'select'; id: string } | { type: 'new' } | { type: 'restore' } };

const FORMATS: SectionFormat[] = ['paragraph', 'list', 'string'];

const KIND_LABEL: Record<TemplateKind, string> = {
  builtin: 'Built-in',
  custom: 'Custom',
  edited: 'Edited',
  new: 'New',
};

const EMPTY_SECTION: TemplateSection = {
  title: '',
  instruction: '',
  format: 'paragraph',
  itemFormat: '',
};

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function asFormat(format: string): SectionFormat {
  return FORMATS.includes(format as SectionFormat) ? (format as SectionFormat) : 'paragraph';
}

function blankDraft(): Draft {
  return {
    id: null,
    kind: 'new',
    name: '',
    description: '',
    sections: [{ ...EMPTY_SECTION }],
  };
}

function draftFromEditor(state: EditorTemplate): Draft {
  return {
    id: state.id,
    kind: state.kind,
    name: state.name,
    description: state.description,
    sections: state.sections.map(section => ({
      title: section.title,
      instruction: section.instruction,
      format: asFormat(section.format),
      itemFormat: section.itemFormat ?? '',
    })),
  };
}

function canonical(draft: Pick<Draft, 'name' | 'description' | 'sections'>): string {
  return JSON.stringify({
    name: draft.name,
    description: draft.description,
    sections: draft.sections.map(section => ({
      title: section.title,
      instruction: section.instruction,
      format: section.format,
      itemFormat: section.format === 'list' ? section.itemFormat : '',
    })),
  });
}

function isComplete(draft: Draft): boolean {
  return (
    draft.name.trim().length > 0 &&
    draft.description.trim().length > 0 &&
    draft.sections.length > 0 &&
    draft.sections.every(section => section.title.trim() && section.instruction.trim())
  );
}

export function SummaryTemplateSettings() {
  const [templates, setTemplates] = useState<TemplateSummary[]>([]);
  const [hiddenCount, setHiddenCount] = useState(0);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [baseline, setBaseline] = useState('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const isDirty = draft !== null && canonical(draft) !== baseline;
  const canSave = draft !== null && isDirty && isComplete(draft) && !isSaving && pending === null;

  const applyDraft = (next: Draft) => {
    setDraft(next);
    setBaseline(canonical(next));
    setPending(null);
  };

  const refreshList = async () => {
    const [listed, hidden] = await Promise.all([
      invoke<TemplateSummary[]>('api_list_templates'),
      invoke<number>('api_hidden_template_count'),
    ]);
    setTemplates(listed);
    setHiddenCount(hidden);
    return listed;
  };

  const loadTemplate = async (templateId: string) => {
    const state = await invoke<EditorTemplate>('api_get_template', { templateId });
    applyDraft(draftFromEditor(state));
  };

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [listed, hidden] = await Promise.all([
          invoke<TemplateSummary[]>('api_list_templates'),
          invoke<number>('api_hidden_template_count'),
        ]);
        if (cancelled) return;
        setTemplates(listed);
        setHiddenCount(hidden);
        if (listed[0]) {
          const state = await invoke<EditorTemplate>('api_get_template', { templateId: listed[0].id });
          if (cancelled) return;
          applyDraft(draftFromEditor(state));
        }
      } catch (error) {
        if (!cancelled) toast.error(`Failed to load templates: ${errorText(error)}`);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const startNew = () => applyDraft(blankDraft());

  const requestSelect = (templateId: string) => {
    if (draft?.id === templateId && draft.kind !== 'new') return;
    if (isDirty) {
      setPending({ type: 'discard', next: { type: 'select', id: templateId } });
      return;
    }
    loadTemplate(templateId).catch(error => toast.error(`Failed to load template: ${errorText(error)}`));
  };

  const requestNew = () => {
    if (draft?.kind === 'new' && !isDirty) return;
    if (isDirty) {
      setPending({ type: 'discard', next: { type: 'new' } });
      return;
    }
    startNew();
  };

  const confirmDiscard = () => {
    if (pending?.type !== 'discard') return;
    const next = pending.next;
    if (next.type === 'select') {
      loadTemplate(next.id).catch(error => toast.error(`Failed to load template: ${errorText(error)}`));
    } else if (next.type === 'new') {
      startNew();
    } else {
      void performRestore();
    }
  };

  const updateDraft = (patch: Partial<Draft>) => {
    setDraft(current => (current ? { ...current, ...patch } : current));
  };

  const updateSection = (index: number, patch: Partial<TemplateSection>) => {
    setDraft(current => {
      if (!current) return current;
      return {
        ...current,
        sections: current.sections.map((section, sectionIndex) =>
          sectionIndex === index ? { ...section, ...patch } : section),
      };
    });
  };

  const moveSection = (index: number, direction: -1 | 1) => {
    setDraft(current => {
      if (!current) return current;
      const nextIndex = index + direction;
      if (nextIndex < 0 || nextIndex >= current.sections.length) return current;
      const sections = [...current.sections];
      const [section] = sections.splice(index, 1);
      sections.splice(nextIndex, 0, section);
      return { ...current, sections };
    });
  };

  const save = async () => {
    if (!draft || !canSave) return;
    setIsSaving(true);
    try {
      const saved = await invoke<EditorTemplate>('api_save_template', {
        id: draft.id,
        template: {
          name: draft.name.trim(),
          description: draft.description.trim(),
          sections: draft.sections.map(section => ({
            title: section.title.trim(),
            instruction: section.instruction.trim(),
            format: section.format,
            itemFormat: section.format === 'list' && section.itemFormat.trim()
              ? section.itemFormat.trim()
              : undefined,
          })),
        },
      });
      await refreshList();
      applyDraft(draftFromEditor(saved));
      toast.success(`Saved ${saved.name}`);
    } catch (error) {
      toast.error(`Failed to save template: ${errorText(error)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const confirmDelete = async () => {
    if (!draft?.id) return;
    const deletedId = draft.id;
    const deletedName = draft.name;
    const deletedKind = draft.kind;
    const index = templates.findIndex(template => template.id === deletedId);
    setIsSaving(true);
    try {
      await invoke('api_delete_template', { templateId: deletedId });
      const listed = await refreshList();
      const neighbor = listed[Math.min(index, listed.length - 1)];
      if (neighbor) {
        await loadTemplate(neighbor.id);
      } else {
        setDraft(null);
        setBaseline('');
        setPending(null);
      }
      toast.success(deletedKind === 'custom' ? `Deleted ${deletedName}` : `Hid ${deletedName}`);
    } catch (error) {
      toast.error(`Failed to delete template: ${errorText(error)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const confirmReset = async () => {
    if (!draft?.id) return;
    setIsSaving(true);
    try {
      const restored = await invoke<EditorTemplate>('api_reset_template', { templateId: draft.id });
      await refreshList();
      applyDraft(draftFromEditor(restored));
      toast.success(`Reset ${restored.name}`);
    } catch (error) {
      toast.error(`Failed to reset template: ${errorText(error)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const performRestore = async () => {
    setIsSaving(true);
    try {
      const restored = await invoke<string[]>('api_restore_builtin_templates');
      const listed = await refreshList();
      const first = listed.find(template => restored.includes(template.id)) ?? listed[0];
      if (first) await loadTemplate(first.id);
      toast.success('Restored built-in templates');
    } catch (error) {
      toast.error(`Failed to restore templates: ${errorText(error)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const restoreHidden = () => {
    if (isDirty) {
      setPending({ type: 'discard', next: { type: 'restore' } });
      return;
    }
    void performRestore();
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm space-y-4">
      <div>
        <div className="flex items-center gap-2 mb-2">
          <FileText className="h-5 w-5 text-gray-600" />
          <h3 className="text-lg font-semibold text-gray-900">Summary templates</h3>
        </div>
        <p className="text-sm text-gray-600">
          These templates decide the sections and instructions used when a summary is generated.
          Pick one from the Template menu on a meeting.
        </p>
      </div>

      <div className="flex flex-col gap-6 lg:flex-row">
        <div className="lg:w-72 shrink-0 space-y-3">
          <div className="space-y-2">
            {templates.length === 0 && (
              <p className="text-sm text-gray-600">No templates yet.</p>
            )}
            {templates.map(template => {
              const selected = draft?.id === template.id && draft.kind !== 'new';
              return (
                <button
                  key={template.id}
                  type="button"
                  aria-current={selected ? 'true' : undefined}
                  onClick={() => requestSelect(template.id)}
                  className={`w-full rounded-md border px-3 py-2 text-left ${
                    selected ? 'border-blue-600 bg-blue-50' : 'border-gray-200 hover:bg-gray-50'
                  }`}
                >
                  <span className="flex items-center justify-between gap-2">
                    <span className="font-medium text-gray-900">{template.name}</span>
                    <span className="shrink-0 rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-600">
                      {KIND_LABEL[template.kind]}
                    </span>
                  </span>
                  <span className="mt-1 block text-sm text-gray-600 line-clamp-2">{template.description}</span>
                </button>
              );
            })}
          </div>
          <div className="flex flex-col gap-2">
            <Button type="button" variant="outline" size="sm" onClick={requestNew}>
              <Plus /> New template
            </Button>
            {hiddenCount > 0 && (
              <Button type="button" variant="outline" size="sm" onClick={restoreHidden} disabled={isSaving}>
                Restore built-in templates
              </Button>
            )}
          </div>
        </div>

        <div className="min-w-0 flex-1">
          {!draft ? (
            <p className="text-sm text-gray-600">
              No templates. Create one or restore the built-in templates.
            </p>
          ) : (
            <div className="space-y-4">
              <div className="flex items-center justify-between gap-2">
                <h4 className="font-medium text-gray-900">{draft.name.trim() || 'New template'}</h4>
                <span className="rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-600">
                  {KIND_LABEL[draft.kind]}
                </span>
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-gray-700" htmlFor="template-name">Name</label>
                <Input
                  id="template-name"
                  aria-label="Template name"
                  value={draft.name}
                  onChange={event => updateDraft({ name: event.target.value })}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium text-gray-700" htmlFor="template-description">Description</label>
                <Textarea
                  id="template-description"
                  aria-label="Template description"
                  value={draft.description}
                  rows={2}
                  onChange={event => updateDraft({ description: event.target.value })}
                />
              </div>

              <div className="space-y-3">
                {draft.sections.map((section, index) => (
                  <div key={index} className="rounded-md border border-gray-200 p-3 space-y-3">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium text-gray-700">Section {index + 1}</span>
                      <div className="flex gap-1">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          aria-label={`Move section ${index + 1} up`}
                          disabled={index === 0}
                          onClick={() => moveSection(index, -1)}
                        >
                          <ChevronUp />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          aria-label={`Move section ${index + 1} down`}
                          disabled={index === draft.sections.length - 1}
                          onClick={() => moveSection(index, 1)}
                        >
                          <ChevronDown />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          aria-label={`Remove section ${index + 1}`}
                          onClick={() => updateDraft({
                            sections: draft.sections.filter((_, sectionIndex) => sectionIndex !== index),
                          })}
                        >
                          <Trash2 />
                        </Button>
                      </div>
                    </div>
                    <Input
                      aria-label={`Section ${index + 1} title`}
                      placeholder="Title"
                      value={section.title}
                      onChange={event => updateSection(index, { title: event.target.value })}
                    />
                    <select
                      aria-label={`Section ${index + 1} format`}
                      className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm"
                      value={section.format}
                      onChange={event => updateSection(index, { format: asFormat(event.target.value) })}
                    >
                      {FORMATS.map(format => (
                        <option key={format} value={format}>{format}</option>
                      ))}
                    </select>
                    <Textarea
                      aria-label={`Section ${index + 1} instruction`}
                      placeholder="Instruction for this section"
                      value={section.instruction}
                      rows={3}
                      onChange={event => updateSection(index, { instruction: event.target.value })}
                    />
                    {section.format === 'list' && (
                      <Textarea
                        aria-label={`Section ${index + 1} item format`}
                        placeholder="Optional item format, such as a markdown table"
                        value={section.itemFormat}
                        rows={2}
                        onChange={event => updateSection(index, { itemFormat: event.target.value })}
                      />
                    )}
                  </div>
                ))}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => updateDraft({ sections: [...draft.sections, { ...EMPTY_SECTION }] })}
                >
                  <Plus /> Add section
                </Button>
              </div>

              {pending?.type === 'discard' && (
                <div className="rounded-md border border-amber-200 bg-amber-50 p-3 space-y-2">
                  <p className="text-sm text-gray-800">Discard unsaved changes?</p>
                  <div className="flex gap-2">
                    <Button type="button" variant="outline" size="sm" onClick={() => setPending(null)}>
                      Keep editing
                    </Button>
                    <Button type="button" size="sm" onClick={confirmDiscard}>Discard</Button>
                  </div>
                </div>
              )}
              {pending?.type === 'delete' && (
                <div className="rounded-md border border-red-200 bg-red-50 p-3 space-y-2">
                  <p className="text-sm text-gray-800">
                    {draft.kind === 'custom'
                      ? `Delete "${draft.name}"? This removes it from the Template menu.`
                      : `Hide "${draft.name}"? You can restore built-in templates later.`}
                  </p>
                  <div className="flex gap-2">
                    <Button type="button" variant="outline" size="sm" onClick={() => setPending(null)}>
                      Cancel
                    </Button>
                    <Button type="button" variant="destructive" size="sm" onClick={confirmDelete} disabled={isSaving}>
                      Delete
                    </Button>
                  </div>
                </div>
              )}
              {pending?.type === 'reset' && (
                <div className="rounded-md border border-amber-200 bg-amber-50 p-3 space-y-2">
                  <p className="text-sm text-gray-800">
                    Reset "{draft.name}" to the built-in version? Your edits will be removed.
                  </p>
                  <div className="flex gap-2">
                    <Button type="button" variant="outline" size="sm" onClick={() => setPending(null)}>
                      Cancel
                    </Button>
                    <Button type="button" size="sm" onClick={confirmReset} disabled={isSaving}>
                      Reset
                    </Button>
                  </div>
                </div>
              )}
              {pending === null && (
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex gap-2">
                    {draft.id && (
                      <Button type="button" variant="outline" onClick={() => setPending({ type: 'delete' })}>
                        Delete
                      </Button>
                    )}
                    {draft.kind === 'edited' && (
                      <Button type="button" variant="outline" onClick={() => setPending({ type: 'reset' })}>
                        Reset
                      </Button>
                    )}
                  </div>
                  <Button type="button" onClick={save} disabled={!canSave}>Save</Button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
