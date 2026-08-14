"use client";

import { useState } from "react";
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu';
import { Copy, Save, Loader2, Download, FileText, FileType, Layers } from 'lucide-react';
import Analytics from '@/lib/analytics';
export type { ExportSection } from '@/hooks/meeting-details/useExportOperations';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onCopy: () => Promise<void>;
  onFind?: () => void;
  onOpenFolder: () => Promise<void>;
  onExportToFile: (section?: ExportSection) => Promise<void>;
  onExportToHtml?: (section?: ExportSection) => Promise<void>;
  onExportToPdf?: (section?: ExportSection) => Promise<void>;
  onExportAllToFile?: (section?: ExportSection) => Promise<void>;
  onExportAllToHtml?: (section?: ExportSection) => Promise<void>;
  isExporting?: boolean;
  hasSummary: boolean;
}

const SECTIONS: { value: ExportSection; label: string }[] = [
  { value: 'full', label: 'Full' },
  { value: 'summary', label: 'Summary only' },
  { value: 'transcript', label: 'Transcript only' },
];

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onCopy,
  onFind,
  onOpenFolder,
  onExportToFile,
  onExportToHtml,
  onExportToPdf,
  onExportAllToFile,
  onExportAllToHtml,
  isExporting = false,
  hasSummary,
}: SummaryUpdaterButtonGroupProps) {
  const [section, setSection] = useState<ExportSection>('full');

  return (
    <ButtonGroup>
      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-green-200' : ""}`}
        title={isSaving ? "Saving" : "Save Changes"}
        onClick={() => {
          Analytics.trackButtonClick('save_changes', 'meeting_details');
          onSave();
        }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden lg:inline">Saving...</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden lg:inline">Save</span>
          </>
        )}
      </Button>

      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title="Copy Summary"
        onClick={() => {
          Analytics.trackButtonClick('copy_summary', 'meeting_details');
          onCopy();
        }}
        disabled={!hasSummary}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden lg:inline">Copy</span>
      </Button>

      {/* Export dropdown: format selector + single/batch export */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            title="Export meeting"
            disabled={isExporting}
            className="cursor-pointer"
          >
            {isExporting ? (
              <Loader2 className="animate-spin" />
            ) : (
              <Download />
            )}
            <span className="hidden lg:inline">Export</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <DropdownMenuLabel>Section</DropdownMenuLabel>
          {SECTIONS.map((s) => (
            <DropdownMenuItem
              key={s.value}
              onClick={() => setSection(s.value)}
              className={section === s.value ? 'bg-accent/10 font-medium' : ''}
            >
              <span className="mr-2">{section === s.value ? '✓' : '○'}</span>
              {s.label}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuLabel>This meeting</DropdownMenuLabel>
          <DropdownMenuItem
            onClick={() => {
              Analytics.trackButtonClick('export_markdown', 'meeting_details');
              onExportToFile(section);
            }}
          >
            <FileText className="mr-2 h-4 w-4" />
            Markdown (.md)
          </DropdownMenuItem>
          {onExportToHtml && (
            <DropdownMenuItem
              onClick={() => {
                Analytics.trackButtonClick('export_html', 'meeting_details');
                onExportToHtml(section);
              }}
            >
              <FileType className="mr-2 h-4 w-4" />
              HTML (.html)
            </DropdownMenuItem>
          )}
          {onExportToPdf && (
            <DropdownMenuItem
              onClick={() => {
                Analytics.trackButtonClick('export_pdf', 'meeting_details');
                onExportToPdf(section);
              }}
            >
              <FileText className="mr-2 h-4 w-4" />
              PDF (print)
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuLabel>Batch</DropdownMenuLabel>
          {onExportAllToFile && (
            <DropdownMenuItem
              onClick={() => {
                Analytics.trackButtonClick('export_all_markdown', 'meeting_details');
                onExportAllToFile(section);
              }}
            >
              <Layers className="mr-2 h-4 w-4" />
              All meetings (.md)
            </DropdownMenuItem>
          )}
          {onExportAllToHtml && (
            <DropdownMenuItem
              onClick={() => {
                Analytics.trackButtonClick('export_all_html', 'meeting_details');
                onExportAllToHtml(section);
              }}
            >
              <Layers className="mr-2 h-4 w-4" />
              All meetings (.html)
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Find button */}
      {/* {onFind && (
        <Button
          variant="outline"
          size="sm"
          title="Find in Summary"
          onClick={() => {
            Analytics.trackButtonClick('find_in_summary', 'meeting_details');
            onFind();
          }}
          disabled={!hasSummary}
          className="cursor-pointer"
        >
          <Search />
          <span className="hidden lg:inline">Find</span>
        </Button>
      )} */}
    </ButtonGroup>
  );
}
