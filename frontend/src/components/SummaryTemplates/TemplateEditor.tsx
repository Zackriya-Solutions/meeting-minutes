"use client";

import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, Loader2, Plus, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

export type SectionFormat = 'paragraph' | 'list' | 'string';

export interface TemplateSectionDraft {
  title: string;
  instruction: string;
  format: SectionFormat;
  item_format?: string;
}

export interface TemplateDraft {
  id: string;
  name: string;
  description: string;
  sections: TemplateSectionDraft[];
}

export function emptyDraft(): TemplateDraft {
  return {
    id: '',
    name: '',
    description: '',
    sections: [{ title: '', instruction: '', format: 'paragraph' }],
  };
}

/** Shape written to disk. `id` is the filename and is not part of the JSON. */
function toTemplateJson(draft: TemplateDraft): string {
  return JSON.stringify(
    {
      name: draft.name.trim(),
      description: draft.description.trim(),
      sections: draft.sections.map((section) => ({
        title: section.title.trim(),
        instruction: section.instruction.trim(),
        format: section.format,
        ...(section.item_format?.trim() ? { item_format: section.item_format.trim() } : {}),
      })),
    },
    null,
    2
  );
}

const ID_PATTERN = /^[a-z0-9_-]+$/;

/** Mirrors the Rust-side id rules so the user sees the problem before saving. */
function validateId(id: string): string | null {
  if (!id) return 'An id is required';
  if (id.length > 64) return 'Ids cannot be longer than 64 characters';
  if (!ID_PATTERN.test(id)) {
    return "Use lowercase letters, digits, '_' and '-' only";
  }
  return null;
}

interface TemplateEditorProps {
  draft: TemplateDraft;
  /** True when editing an existing custom template: the id cannot change. */
  idLocked: boolean;
  onSaved: (templateId: string) => void;
  onCancel: () => void;
}

export function TemplateEditor({ draft, idLocked, onSaved, onCancel }: TemplateEditorProps) {
  const [value, setValue] = useState<TemplateDraft>(draft);
  const [rawJson, setRawJson] = useState<string>(() => toTemplateJson(draft));
  const [isSaving, setIsSaving] = useState(false);

  const idError = useMemo(() => validateId(value.id), [value.id]);

  const update = (patch: Partial<TemplateDraft>) => setValue((prev) => ({ ...prev, ...patch }));

  const updateSection = (index: number, patch: Partial<TemplateSectionDraft>) => {
    setValue((prev) => ({
      ...prev,
      sections: prev.sections.map((section, i) =>
        i === index ? { ...section, ...patch } : section
      ),
    }));
  };

  const moveSection = (index: number, delta: number) => {
    const target = index + delta;
    setValue((prev) => {
      if (target < 0 || target >= prev.sections.length) return prev;
      const sections = [...prev.sections];
      [sections[index], sections[target]] = [sections[target], sections[index]];
      return { ...prev, sections };
    });
  };

  const removeSection = (index: number) => {
    setValue((prev) => ({
      ...prev,
      sections: prev.sections.filter((_, i) => i !== index),
    }));
  };

  const addSection = () => {
    setValue((prev) => ({
      ...prev,
      sections: [...prev.sections, { title: '', instruction: '', format: 'paragraph' }],
    }));
  };

  /** Replaces the structured form from hand-edited JSON. */
  const applyRawJson = () => {
    try {
      const parsed = JSON.parse(rawJson) as Partial<TemplateDraft>;
      if (!Array.isArray(parsed.sections)) {
        throw new Error('"sections" must be an array');
      }
      update({
        name: typeof parsed.name === 'string' ? parsed.name : '',
        description: typeof parsed.description === 'string' ? parsed.description : '',
        sections: parsed.sections.map((section) => ({
          title: String(section?.title ?? ''),
          instruction: String(section?.instruction ?? ''),
          format: (section?.format ?? 'paragraph') as SectionFormat,
          item_format: section?.item_format ? String(section.item_format) : undefined,
        })),
      });
      toast.success('JSON applied to the form');
    } catch (error) {
      toast.error('Could not parse JSON', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const handleSave = async () => {
    if (idError) {
      toast.error('Fix the template id first', { description: idError });
      return;
    }

    const templateJson = toTemplateJson(value);
    setIsSaving(true);
    try {
      // The backend is the authority on the schema; check before writing so the
      // error message comes from one place.
      await invoke('api_validate_template', { templateJson });
      await invoke('api_save_custom_template', { templateId: value.id, templateJson });
      toast.success(`Saved "${value.name.trim()}"`);
      onSaved(value.id);
    } catch (error) {
      toast.error('Could not save the template', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Tabs
        defaultValue="form"
        onValueChange={(tab) => {
          // Entering the JSON tab always shows the current form state.
          if (tab === 'json') setRawJson(toTemplateJson(value));
        }}
      >
        <TabsList>
          <TabsTrigger value="form">Form</TabsTrigger>
          <TabsTrigger value="json">Raw JSON</TabsTrigger>
        </TabsList>

        <TabsContent value="form" className="flex flex-col gap-4 pt-2">
          <div className="grid gap-2">
            <Label htmlFor="template-id">Template id</Label>
            <Input
              id="template-id"
              value={value.id}
              disabled={idLocked}
              placeholder="weekly_sync"
              onChange={(e) => update({ id: e.target.value.trim().toLowerCase() })}
            />
            <p className={`text-xs ${idError && !idLocked ? 'text-red-600' : 'text-gray-500'}`}>
              {idLocked
                ? 'The id of an existing template cannot be changed. Duplicate it instead.'
                : idError ?? 'Used as the filename. Lowercase letters, digits, "_" and "-".'}
            </p>
          </div>

          <div className="grid gap-2">
            <Label htmlFor="template-name">Name</Label>
            <Input
              id="template-name"
              value={value.name}
              placeholder="Weekly Sync"
              onChange={(e) => update({ name: e.target.value })}
            />
          </div>

          <div className="grid gap-2">
            <Label htmlFor="template-description">Description</Label>
            <Input
              id="template-description"
              value={value.description}
              placeholder="What this template is for"
              onChange={(e) => update({ description: e.target.value })}
            />
          </div>

          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <Label>Sections</Label>
              <Button variant="outline" size="sm" onClick={addSection}>
                <Plus size={14} />
                Add section
              </Button>
            </div>

            {value.sections.map((section, index) => (
              <div key={index} className="rounded-md border border-gray-200 p-3 flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <Input
                    value={section.title}
                    placeholder="Section title, e.g. Action Items"
                    onChange={(e) => updateSection(index, { title: e.target.value })}
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    title="Move up"
                    disabled={index === 0}
                    onClick={() => moveSection(index, -1)}
                  >
                    <ArrowUp size={14} />
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    title="Move down"
                    disabled={index === value.sections.length - 1}
                    onClick={() => moveSection(index, 1)}
                  >
                    <ArrowDown size={14} />
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    title="Remove section"
                    disabled={value.sections.length === 1}
                    onClick={() => removeSection(index)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>

                <Textarea
                  value={section.instruction}
                  placeholder="What the model should extract for this section"
                  rows={2}
                  onChange={(e) => updateSection(index, { instruction: e.target.value })}
                />

                <div className="flex items-center gap-2">
                  <select
                    className="h-9 rounded-md border border-gray-200 bg-white px-2 text-sm"
                    value={section.format}
                    onChange={(e) =>
                      updateSection(index, { format: e.target.value as SectionFormat })
                    }
                  >
                    <option value="paragraph">paragraph</option>
                    <option value="list">list</option>
                    <option value="string">string</option>
                  </select>
                  <Input
                    value={section.item_format ?? ''}
                    placeholder="Optional item format, e.g. a markdown table row"
                    onChange={(e) => updateSection(index, { item_format: e.target.value })}
                  />
                </div>
              </div>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="json" className="flex flex-col gap-2 pt-2">
          <Textarea
            value={rawJson}
            rows={18}
            spellCheck={false}
            className="font-mono text-xs"
            onChange={(e) => setRawJson(e.target.value)}
          />
          <div>
            <Button variant="outline" size="sm" onClick={applyRawJson}>
              Apply JSON to form
            </Button>
          </div>
        </TabsContent>
      </Tabs>

      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={onCancel} disabled={isSaving}>
          Cancel
        </Button>
        <Button onClick={handleSave} disabled={isSaving}>
          {isSaving ? <Loader2 className="animate-spin" size={14} /> : null}
          Save template
        </Button>
      </div>
    </div>
  );
}
