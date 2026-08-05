'use client';

import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Pencil, RotateCcw, Trash2, Plus, X, Download, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  is_bundled: boolean;
}

interface TemplateSectionFull {
  title: string;
  instruction: string;
  format: string;
  item_format?: string;
  example_item_format?: string;
}

interface TemplateFullDetails {
  id: string;
  name: string;
  description: string;
  sections: TemplateSectionFull[];
}

const FORMAT_OPTIONS = ['paragraph', 'list', 'string'] as const;

const EMPTY_SECTION: TemplateSectionFull = {
  title: '',
  instruction: '',
  format: 'paragraph',
};

function SectionEditor({
  section,
  index,
  onChange,
  onRemove,
}: {
  section: TemplateSectionFull;
  index: number;
  onChange: (index: number, updated: TemplateSectionFull) => void;
  onRemove: (index: number) => void;
}) {
  return (
    <div className="border border-gray-200 rounded-md p-4 space-y-3 relative">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium text-gray-400 uppercase tracking-wide w-6">{index + 1}</span>
        <input
          type="text"
          value={section.title}
          onChange={(e) => onChange(index, { ...section, title: e.target.value })}
          placeholder="Section title"
          className="flex-1 text-sm font-medium border-0 border-b border-gray-200 focus:border-blue-400 focus:outline-none py-1 bg-transparent"
        />
        <Select
          value={section.format}
          onValueChange={(val) => onChange(index, { ...section, format: val })}
        >
          <SelectTrigger className="w-28 h-7 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {FORMAT_OPTIONS.map((f) => (
              <SelectItem key={f} value={f} className="text-xs">
                {f}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <button
          type="button"
          onClick={() => onRemove(index)}
          className="text-gray-300 hover:text-red-400 transition-colors ml-1"
          title="Remove section"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
      <textarea
        value={section.instruction}
        onChange={(e) => onChange(index, { ...section, instruction: e.target.value })}
        placeholder="Instruction for the AI on what to extract for this section"
        rows={3}
        className="w-full text-sm border border-gray-200 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-blue-400 resize-y bg-white"
      />
    </div>
  );
}

const EMPTY_TEMPLATE: TemplateFullDetails = {
  id: '',
  name: '',
  description: '',
  sections: [{ ...EMPTY_SECTION }],
};

export function TemplateManagementSettings() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingTemplate, setEditingTemplate] = useState<TemplateFullDetails | null>(null);
  const [isNewTemplate, setIsNewTemplate] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<{ id: string; name: string } | null>(null);
  const importInputRef = useRef<HTMLInputElement>(null);

  const fetchTemplates = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<TemplateInfo[]>('api_list_templates');
      setTemplates(result);
    } catch (err) {
      console.error('Failed to load templates:', err);
      toast.error('Failed to load templates');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchTemplates();
  }, [fetchTemplates]);

  const openEditor = async (templateId: string) => {
    try {
      const full = await invoke<TemplateFullDetails>('api_get_template_full', { templateId });
      setEditingTemplate(full);
      setIsNewTemplate(false);
      setDialogOpen(true);
    } catch (err) {
      console.error('Failed to load template details:', err);
      toast.error('Failed to open template editor');
    }
  };

  const openNewTemplate = () => {
    setEditingTemplate({ ...EMPTY_TEMPLATE, sections: [{ ...EMPTY_SECTION }] });
    setIsNewTemplate(true);
    setDialogOpen(true);
  };

  const handleSectionChange = (index: number, updated: TemplateSectionFull) => {
    if (!editingTemplate) return;
    const sections = [...editingTemplate.sections];
    sections[index] = updated;
    setEditingTemplate({ ...editingTemplate, sections });
  };

  const handleAddSection = () => {
    if (!editingTemplate) return;
    setEditingTemplate({
      ...editingTemplate,
      sections: [...editingTemplate.sections, { ...EMPTY_SECTION }],
    });
  };

  const handleRemoveSection = (index: number) => {
    if (!editingTemplate) return;
    const sections = editingTemplate.sections.filter((_, i) => i !== index);
    setEditingTemplate({ ...editingTemplate, sections });
  };

  const handleSave = async () => {
    if (!editingTemplate) return;

    if (isNewTemplate) {
      const id = editingTemplate.id.trim();
      if (!id) {
        toast.error('Template ID is required');
        return;
      }
      if (!/^[a-z0-9_]+$/.test(id)) {
        toast.error('Template ID may only contain lowercase letters, digits, and underscores');
        return;
      }
      if (templates.some((t) => t.id === id)) {
        toast.error('A template with this ID already exists');
        return;
      }
    }

    if (!editingTemplate.name.trim()) {
      toast.error('Template name is required');
      return;
    }
    if (editingTemplate.sections.length === 0) {
      toast.error('At least one section is required');
      return;
    }

    setSaving(true);
    try {
      const templateId = isNewTemplate ? editingTemplate.id.trim() : editingTemplate.id;
      const json = JSON.stringify(
        {
          name: editingTemplate.name,
          description: editingTemplate.description,
          sections: editingTemplate.sections,
        },
        null,
        2
      );
      await invoke('api_save_custom_template', { templateId, templateJson: json });
      toast.success(isNewTemplate ? 'Template created' : 'Template saved', {
        description: `"${editingTemplate.name}" has been ${isNewTemplate ? 'created' : 'saved'}.`,
      });
      setDialogOpen(false);
      fetchTemplates();
    } catch (err) {
      console.error('Failed to save template:', err);
      toast.error('Failed to save template', {
        description: typeof err === 'string' ? err : 'Validation or file system error.',
      });
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async (templateId: string, templateName: string) => {
    try {
      const wasDeleted = await invoke<boolean>('api_delete_custom_template', { templateId });
      if (wasDeleted) {
        toast.success('Template reset', {
          description: `"${templateName}" has been reset to the default.`,
        });
      } else {
        toast.info('No custom changes to reset', {
          description: `"${templateName}" is already using the default.`,
        });
      }
      fetchTemplates();
    } catch (err) {
      console.error('Failed to reset template:', err);
      toast.error('Failed to reset template');
    }
  };

  const handleDelete = (templateId: string, templateName: string) => {
    setDeleteConfirm({ id: templateId, name: templateName });
  };

  const confirmDelete = async () => {
    if (!deleteConfirm) return;
    const { id, name } = deleteConfirm;
    setDeleteConfirm(null);
    try {
      await invoke<boolean>('api_delete_custom_template', { templateId: id });
      toast.success('Template deleted', {
        description: `"${name}" has been deleted.`,
      });
      fetchTemplates();
    } catch (err) {
      console.error('Failed to delete template:', err);
      toast.error('Failed to delete template');
    }
  };

  const handleExport = async (templateId: string, templateName: string) => {
    try {
      const full = await invoke<TemplateFullDetails>('api_get_template_full', { templateId });
      const exportData = {
        name: full.name,
        description: full.description,
        sections: full.sections,
      };
      const json = JSON.stringify(exportData, null, 2);
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${templateId}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success('Template exported', {
        description: `"${templateName}" saved as ${templateId}.json`,
      });
    } catch (err) {
      console.error('Failed to export template:', err);
      toast.error('Failed to export template');
    }
  };

  const handleImportFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    // Reset so the same file can be re-imported later
    e.target.value = '';

    const reader = new FileReader();
    reader.onload = (ev) => {
      try {
        const json = ev.target?.result as string;
        const parsed = JSON.parse(json);

        // Basic shape check
        if (typeof parsed.name !== 'string' || !Array.isArray(parsed.sections)) {
          toast.error('Invalid template file', {
            description: 'File must contain "name" (string) and "sections" (array).',
          });
          return;
        }

        // Derive an initial ID from the file name (strip .json, lowercase, replace non-alnum with _)
        const rawName = file.name.replace(/\.json$/i, '');
        const suggestedId = rawName.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '');

        const draft: TemplateFullDetails = {
          id: suggestedId,
          name: parsed.name || '',
          description: parsed.description || '',
          sections: (parsed.sections as any[]).map((s) => ({
            title: s.title || '',
            instruction: s.instruction || '',
            format: ['paragraph', 'list', 'string'].includes(s.format) ? s.format : 'paragraph',
            item_format: s.item_format,
            example_item_format: s.example_item_format,
          })),
        };

        if (draft.sections.length === 0) {
          draft.sections = [{ ...EMPTY_SECTION }];
        }

        setEditingTemplate(draft);
        setIsNewTemplate(true);
        setDialogOpen(true);
        toast.info('Template imported — review and save', {
          description: 'The template has been loaded into the editor. Adjust the ID if needed.',
        });
      } catch {
        toast.error('Failed to parse template file', {
          description: 'Make sure the file is valid JSON.',
        });
      }
    };
    reader.readAsText(file);
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between mb-1">
        <h3 className="text-lg font-semibold text-gray-900">Summary Templates</h3>
        <div className="flex items-center gap-2">
          <input
            ref={importInputRef}
            type="file"
            accept=".json,application/json"
            className="hidden"
            onChange={handleImportFile}
          />
          <Button
            variant="outline"
            size="sm"
            className="h-8 px-3 text-xs gap-1"
            onClick={() => importInputRef.current?.click()}
          >
            <Upload className="w-3 h-3" />
            Import
          </Button>
          <Button variant="outline" size="sm" className="h-8 px-3 text-xs gap-1" onClick={openNewTemplate}>
            <Plus className="w-3 h-3" />
            New Template
          </Button>
        </div>
      </div>
      <p className="text-sm text-gray-600 mb-4">
        Edit the instructions each template section gives to the AI. Default templates can be reset; custom templates can be deleted.
      </p>

      {loading ? (
        <p className="text-sm text-gray-400">Loading templates…</p>
      ) : (
        <div className="space-y-2">
          {templates.map((tmpl) => (
            <div
              key={tmpl.id}
              className="flex items-center justify-between rounded-md border border-gray-100 px-4 py-3 hover:border-gray-200 transition-colors"
            >
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium text-gray-800 truncate">{tmpl.name}</p>
                  {!tmpl.is_bundled && (
                    <span className="text-xs px-1.5 py-0.5 rounded bg-blue-50 text-blue-600 font-medium shrink-0">
                      custom
                    </span>
                  )}
                </div>
                <p className="text-xs text-gray-500 truncate">{tmpl.description}</p>
              </div>
              <div className="flex items-center gap-2 ml-4 shrink-0">
                {tmpl.is_bundled ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-gray-500 hover:text-gray-700"
                    onClick={() => handleReset(tmpl.id, tmpl.name)}
                  >
                    <RotateCcw className="w-3 h-3 mr-1" />
                    Reset
                  </Button>
                ) : (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-red-400 hover:text-red-600"
                    onClick={() => handleDelete(tmpl.id, tmpl.name)}
                  >
                    <Trash2 className="w-3 h-3 mr-1" />
                    Delete
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-xs text-gray-500 hover:text-gray-700"
                  onClick={() => handleExport(tmpl.id, tmpl.name)}
                  title="Export template as JSON"
                >
                  <Download className="w-3 h-3" />
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 px-3 text-xs"
                  onClick={() => openEditor(tmpl.id)}
                >
                  <Pencil className="w-3 h-3 mr-1" />
                  Edit
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="max-w-2xl max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>
              {isNewTemplate ? 'New Template' : `Edit Template: ${editingTemplate?.name}`}
            </DialogTitle>
          </DialogHeader>

          {editingTemplate && (
            <div className="space-y-4 py-2">
              {isNewTemplate && (
                <div>
                  <label className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                    Template ID <span className="text-gray-400 normal-case">(lowercase, underscores only)</span>
                  </label>
                  <input
                    type="text"
                    value={editingTemplate.id}
                    onChange={(e) =>
                      setEditingTemplate({ ...editingTemplate, id: e.target.value })
                    }
                    placeholder="e.g. my_meeting_type"
                    className="mt-1 w-full text-sm border border-gray-200 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-blue-400 font-mono"
                  />
                </div>
              )}
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                  Template Name
                </label>
                <input
                  type="text"
                  value={editingTemplate.name}
                  onChange={(e) =>
                    setEditingTemplate({ ...editingTemplate, name: e.target.value })
                  }
                  className="mt-1 w-full text-sm border border-gray-200 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-blue-400"
                />
              </div>
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                  Description
                </label>
                <input
                  type="text"
                  value={editingTemplate.description}
                  onChange={(e) =>
                    setEditingTemplate({ ...editingTemplate, description: e.target.value })
                  }
                  className="mt-1 w-full text-sm border border-gray-200 rounded-md px-3 py-2 focus:outline-none focus:ring-1 focus:ring-blue-400"
                />
              </div>
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase tracking-wide mb-2 block">
                  Sections
                </label>
                <div className="space-y-3">
                  {editingTemplate.sections.map((section, i) => (
                    <SectionEditor
                      key={i}
                      section={section}
                      index={i}
                      onChange={handleSectionChange}
                      onRemove={handleRemoveSection}
                    />
                  ))}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="mt-3 h-7 px-3 text-xs text-gray-500 hover:text-gray-700 border border-dashed border-gray-300 w-full"
                  onClick={handleAddSection}
                >
                  <Plus className="w-3 h-3 mr-1" />
                  Add Section
                </Button>
              </div>
            </div>
          )}

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDialogOpen(false)}
              disabled={saving}
            >
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving ? 'Saving…' : isNewTemplate ? 'Create Template' : 'Save Template'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation dialog */}
      <Dialog open={!!deleteConfirm} onOpenChange={(open) => { if (!open) setDeleteConfirm(null); }}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Delete template?</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-gray-600">
            <span className="font-medium">"{deleteConfirm?.name}"</span> will be permanently deleted.
            This cannot be undone.
          </p>
          <DialogFooter className="gap-2 sm:gap-0">
            <Button variant="outline" onClick={() => setDeleteConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={confirmDelete}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
