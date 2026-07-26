"use client";

import { useState } from 'react';
import { Download, Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import Analytics from '@/lib/analytics';
import {
  ExportFormat,
  ExportScope,
  FORMAT_LABELS,
  SCOPE_LABELS,
} from '@/lib/export-markdown';

const FORMATS: ExportFormat[] = ['md', 'pdf', 'docx'];
const SCOPES: ExportScope[] = ['summary', 'transcript', 'both'];

interface ExportMenuProps {
  hasSummary: boolean;
  hasTranscripts: boolean;
  onExport: (scope: ExportScope, format: ExportFormat) => Promise<void>;
}

export function ExportMenu({ hasSummary, hasTranscripts, onExport }: ExportMenuProps) {
  const [isExporting, setIsExporting] = useState(false);

  const scopeDisabledReason = (scope: ExportScope): string | null => {
    const needsSummary = scope === 'summary' || scope === 'both';
    const needsTranscript = scope === 'transcript' || scope === 'both';

    if (needsSummary && !hasSummary) return 'Generate a summary first';
    if (needsTranscript && !hasTranscripts) return 'No transcript available';
    return null;
  };

  const runExport = async (scope: ExportScope, format: ExportFormat) => {
    Analytics.trackButtonClick(`export_${scope}_${format}`, 'meeting_details');
    setIsExporting(true);
    try {
      await onExport(scope, format);
    } finally {
      setIsExporting(false);
    }
  };

  const nothingToExport = !hasSummary && !hasTranscripts;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          disabled={nothingToExport || isExporting}
          title={nothingToExport ? 'Nothing to export yet' : 'Export this meeting'}
        >
          {isExporting ? <Loader2 className="animate-spin" /> : <Download />}
          <span className="hidden lg:inline">Export</span>
        </Button>
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end">
        {FORMATS.map((format) => (
          <DropdownMenuSub key={format}>
            <DropdownMenuSubTrigger>{FORMAT_LABELS[format]}</DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              {SCOPES.map((scope) => {
                const disabledReason = scopeDisabledReason(scope);
                return (
                  <DropdownMenuItem
                    key={scope}
                    disabled={disabledReason !== null}
                    title={disabledReason ?? `Export the ${SCOPE_LABELS[scope].toLowerCase()}`}
                    onClick={() => runExport(scope, format)}
                  >
                    {SCOPE_LABELS[scope]}
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
