"use client";

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Copy, Loader2, Pencil, Plus, RefreshCw, Trash2, Upload } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  emptyDraft,
  SectionFormat,
  TemplateDraft,
  TemplateEditor,
} from './TemplateEditor';

type TemplateSource = 'builtin' | 'bundled' | 'custom';

interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  source: TemplateSource;
  editable: boolean;
}

interface TemplateWithSource {
  id: string;
  name: string;
  description: string;
  sections: Array<{
    title: string;
    instruction: string;
    format: SectionFormat;
    item_format?: string | null;
  }>;
  source: TemplateSource;
  editable: boolean;
  suggested_copy_id: string;
}

const SOURCE_LABELS: Record<TemplateSource, string> = {
  builtin: 'Built-in',
  bundled: 'Bundled',
  custom: 'Custom',
};

function toDraft(template: TemplateWithSource, id: string): TemplateDraft {
  return {
    id,
    name: template.name,
    description: template.description,
    sections: template.sections.map((section) => ({
      title: section.title,
      instruction: section.instruction,
      format: section.format,
      item_format: section.item_format ?? undefined,
    })),
  };
}

export function TemplateManager() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [editing, setEditing] = useState<{ draft: TemplateDraft; idLocked: boolean } | null>(
    null
  );
  const [busyId, setBusyId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    setIsLoading(true);
    try {
      setTemplates(await invoke<TemplateInfo[]>('api_list_templates'));
    } catch (error) {
      console.error('Failed to list templates:', error);
      toast.error('Could not load templates', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const openForEdit = async (id: string, duplicate: boolean) => {
    setBusyId(id);
    try {
      const template = await invoke<TemplateWithSource>('api_get_template_source', {
        templateId: id,
      });
      setEditing({
        draft: toDraft(template, duplicate ? template.suggested_copy_id : id),
        idLocked: !duplicate,
      });
    } catch (error) {
      toast.error('Could not open the template', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (template: TemplateInfo) => {
    // Deleting a template is irreversible and the file lives outside the app.
    const confirmed = window.confirm(`Delete the custom template "${template.name}"?`);
    if (!confirmed) return;

    setBusyId(template.id);
    try {
      await invoke('api_delete_custom_template', { templateId: template.id });
      toast.success(`Deleted "${template.name}"`);
      await load();
    } catch (error) {
      toast.error('Could not delete the template', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setBusyId(null);
    }
  };

  const handleCopyJson = async (id: string) => {
    try {
      const template = await invoke<TemplateWithSource>('api_get_template_source', {
        templateId: id,
      });
      await navigator.clipboard.writeText(
        JSON.stringify(
          {
            name: template.name,
            description: template.description,
            sections: template.sections,
          },
          null,
          2
        )
      );
      toast.success('Template JSON copied to clipboard');
    } catch (error) {
      toast.error('Could not copy the template', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const handleImportFile = async (file: File) => {
    try {
      const parsed = JSON.parse(await file.text()) as Partial<TemplateWithSource>;
      if (!Array.isArray(parsed.sections)) {
        throw new Error('The file has no "sections" array');
      }
      // Seed the id from the filename; the user can change it before saving.
      const suggestedId = file.name
        .replace(/\.json$/i, '')
        .toLowerCase()
        .replace(/[^a-z0-9_-]+/g, '_')
        .replace(/^_+|_+$/g, '');

      setEditing({
        draft: {
          id: suggestedId,
          name: parsed.name ?? '',
          description: parsed.description ?? '',
          sections: parsed.sections.map((section) => ({
            title: String(section?.title ?? ''),
            instruction: String(section?.instruction ?? ''),
            format: (section?.format ?? 'paragraph') as SectionFormat,
            item_format: section?.item_format ?? undefined,
          })),
        },
        idLocked: false,
      });
    } catch (error) {
      toast.error('Could not import the file', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  if (editing) {
    return (
      <div className="flex flex-col gap-4 p-1">
        <h3 className="text-sm font-medium text-gray-700">
          {editing.idLocked ? 'Edit template' : 'New template'}
        </h3>
        <TemplateEditor
          draft={editing.draft}
          idLocked={editing.idLocked}
          onCancel={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await load();
          }}
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-1">
      <div className="flex items-center justify-between">
        <p className="text-sm text-gray-600">
          Templates shape the sections the AI writes. Built-in ones are read-only — duplicate
          one to customise it.
        </p>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => void load()} title="Reload">
            <RefreshCw size={14} />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => fileInputRef.current?.click()}
            title="Import a template JSON file"
          >
            <Upload size={14} />
            Import
          </Button>
          <Button
            size="sm"
            onClick={() => setEditing({ draft: emptyDraft(), idLocked: false })}
          >
            <Plus size={14} />
            New
          </Button>
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        accept="application/json,.json"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          // Reset so selecting the same file twice fires onChange again.
          e.target.value = '';
          if (file) void handleImportFile(file);
        }}
      />

      {isLoading ? (
        <div className="flex items-center gap-2 text-sm text-gray-500 py-6">
          <Loader2 className="animate-spin" size={16} />
          Loading templates...
        </div>
      ) : templates.length === 0 ? (
        <p className="text-sm text-gray-500 py-6">No templates found.</p>
      ) : (
        <div className="flex flex-col divide-y divide-gray-100 rounded-md border border-gray-200">
          {templates.map((template) => (
            <div key={template.id} className="flex items-center gap-3 p-3">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm truncate">{template.name}</span>
                  <span
                    className={`text-[10px] uppercase tracking-wide rounded px-1.5 py-0.5 ${
                      template.source === 'custom'
                        ? 'bg-green-100 text-green-700'
                        : 'bg-gray-100 text-gray-600'
                    }`}
                  >
                    {SOURCE_LABELS[template.source]}
                  </span>
                </div>
                <p className="text-xs text-gray-500 truncate">{template.description}</p>
                <p className="text-[10px] text-gray-400 font-mono">{template.id}</p>
              </div>

              <div className="flex gap-1 shrink-0">
                <Button
                  variant="outline"
                  size="sm"
                  title="Copy JSON to clipboard"
                  onClick={() => void handleCopyJson(template.id)}
                >
                  <Copy size={14} />
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  title="Duplicate as a custom template"
                  disabled={busyId === template.id}
                  onClick={() => void openForEdit(template.id, true)}
                >
                  <Plus size={14} />
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  title={
                    template.editable
                      ? 'Edit this template'
                      : 'Built-in templates are read-only — duplicate it instead'
                  }
                  disabled={!template.editable || busyId === template.id}
                  onClick={() => void openForEdit(template.id, false)}
                >
                  <Pencil size={14} />
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  title={
                    template.editable
                      ? 'Delete this template'
                      : 'Built-in templates cannot be deleted'
                  }
                  disabled={!template.editable || busyId === template.id}
                  onClick={() => void handleDelete(template)}
                >
                  <Trash2 size={14} />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
