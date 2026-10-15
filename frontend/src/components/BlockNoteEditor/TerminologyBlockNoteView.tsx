"use client";

import { useState } from "react";
import type { BlockNoteEditor } from "@blocknote/core";
import {
  FormattingToolbar,
  FormattingToolbarController,
  getFormattingToolbarItems,
  useBlockNoteEditor,
  useComponentsContext,
} from "@blocknote/react";
import { BlockNoteView } from "@blocknote/shadcn";
import { SpellCheck } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { addTerminologyEntry, buildPhraseRegExp } from "@/lib/terminology";

interface SelectionToCorrect {
  text: string;
  from: number;
  to: number;
}

type AnyBlockNoteEditor = BlockNoteEditor<any, any, any>;

function CorrectSpellingButton({ onRequest }: { onRequest: (selection: SelectionToCorrect) => void }) {
  const editor = useBlockNoteEditor<any, any, any>();
  const Components = useComponentsContext()!;
  const text = editor.getSelectedText().trim();
  const isSingleLine = text.length > 0 && !text.includes("\n");

  return (
    <Components.FormattingToolbar.Button
      mainTooltip="Correct spelling"
      label="Correct spelling"
      icon={<SpellCheck size={18} />}
      isDisabled={!isSingleLine}
      onClick={() => {
        const { from, to } = editor.prosemirrorState.selection;
        const selectedText = editor.getSelectedText();
        // Collapsing the selection closes the formatting toolbar so it does not sit above the dialog.
        editor.setTextCursorPosition(editor.getTextCursorPosition().block, "end");
        onRequest({ text: selectedText, from, to });
      }}
    />
  );
}

/** Replaces the selected range only. */
function replaceSelection(editor: AnyBlockNoteEditor, selection: SelectionToCorrect, replacement: string) {
  editor.transact((tr) => {
    tr.insertText(replacement, selection.from, selection.to);
  });
}

/** Replaces every whole-word occurrence of `phrase` in the document; returns the count. */
function replaceAllInDocument(
  editor: AnyBlockNoteEditor,
  phrase: string,
  replacement: string,
  matchCase: boolean,
): number {
  const regex = buildPhraseRegExp(phrase, matchCase);
  const ranges: { from: number; to: number }[] = [];

  editor.prosemirrorState.doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;
    for (const match of node.text.matchAll(regex)) {
      if (match[0] === replacement || match.index === undefined) continue;
      ranges.push({ from: pos + match.index, to: pos + match.index + match[0].length });
    }
  });

  if (ranges.length === 0) return 0;

  editor.transact((tr) => {
    // Apply from the end so earlier positions stay valid.
    for (const range of ranges.reverse()) {
      tr.insertText(replacement, range.from, range.to);
    }
  });
  return ranges.length;
}

interface TerminologyBlockNoteViewProps {
  editor: AnyBlockNoteEditor;
  editable?: boolean;
  onChange?: () => void;
}

/**
 * BlockNoteView with a "Correct spelling" action in the formatting toolbar. Selected text can be
 * replaced once, replaced throughout the summary, or saved to the Terminology list so future
 * summaries are corrected automatically.
 */
export function TerminologyBlockNoteView({ editor, editable = true, onChange }: TerminologyBlockNoteViewProps) {
  const [selection, setSelection] = useState<SelectionToCorrect | null>(null);
  const [replacement, setReplacement] = useState("");
  const [matchCase, setMatchCase] = useState(false);

  const openDialog = (next: SelectionToCorrect) => {
    setSelection(next);
    setReplacement(next.text.trim());
    setMatchCase(false);
  };

  const close = () => setSelection(null);
  const phrase = selection?.text.trim() ?? "";
  const target = replacement.trim();
  const canApply = phrase.length > 0 && target.length > 0 && target !== phrase;

  const handleReplaceOnce = () => {
    if (!selection || !canApply) return;
    replaceSelection(editor, selection, target);
    close();
  };

  const handleReplaceAll = async (saveToTerminology: boolean) => {
    if (!canApply) return;
    const count = replaceAllInDocument(editor, phrase, target, matchCase);
    close();

    if (!saveToTerminology) {
      toast.success(`Replaced ${count} ${count === 1 ? "occurrence" : "occurrences"}`);
      return;
    }

    try {
      await addTerminologyEntry({ wrong: phrase, correct: target, matchCase });
      toast.success(`Replaced ${count} and added "${phrase}" → "${target}" to Terminology`);
    } catch (error) {
      toast.error(`Replaced ${count}, but saving to Terminology failed: ${error}`);
    }
  };

  return (
    <>
      <BlockNoteView
        editor={editor}
        editable={editable}
        onChange={onChange}
        formattingToolbar={false}
        theme="light"
      >
        <FormattingToolbarController
          formattingToolbar={() => (
            <FormattingToolbar>
              {getFormattingToolbarItems()}
              {editable && <CorrectSpellingButton key="correctSpellingButton" onRequest={openDialog} />}
            </FormattingToolbar>
          )}
        />
      </BlockNoteView>

      <Dialog open={selection !== null} onOpenChange={(open) => !open && close()}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Correct spelling</DialogTitle>
            <DialogDescription>
              Replace &ldquo;{phrase}&rdquo; in this summary, or save it to Terminology so future summaries
              are corrected automatically.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="terminology-replacement">Replace with</Label>
              <Input
                id="terminology-replacement"
                autoFocus
                value={replacement}
                onChange={(e) => setReplacement(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleReplaceAll(true)}
              />
            </div>
            <div className="flex items-center justify-between">
              <Label htmlFor="terminology-match-case">Match case</Label>
              <Switch id="terminology-match-case" checked={matchCase} onCheckedChange={setMatchCase} />
            </div>
          </div>

          <DialogFooter className="flex-wrap gap-2 sm:space-x-0">
            <Button variant="outline" onClick={handleReplaceOnce} disabled={!canApply}>
              Replace this one
            </Button>
            <Button variant="outline" onClick={() => handleReplaceAll(false)} disabled={!canApply}>
              Replace all in summary
            </Button>
            <Button onClick={() => handleReplaceAll(true)} disabled={!canApply}>
              Replace all &amp; add to Terminology
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
