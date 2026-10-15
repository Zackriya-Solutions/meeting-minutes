"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { toast } from "sonner"
import { BookA, Plus, Trash2, Upload, Copy, Search, Save, ArrowRight } from "lucide-react"
import { Button } from "./ui/button"
import { Input } from "./ui/input"
import { Switch } from "./ui/switch"
import {
  TerminologyEntry,
  getTerminology,
  saveTerminology,
  parseTerminologyImport,
  serializeTerminology,
} from "@/lib/terminology"

const EMPTY_ENTRY: TerminologyEntry = { wrong: "", correct: "", matchCase: false }

export function TerminologySettings() {
  const [entries, setEntries] = useState<TerminologyEntry[]>([])
  const [draft, setDraft] = useState<TerminologyEntry>(EMPTY_ENTRY)
  const [filter, setFilter] = useState("")
  const [isDirty, setIsDirty] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    getTerminology()
      .then(setEntries)
      .catch((error) => toast.error(`Failed to load terminology: ${error}`))
  }, [])

  const visibleIndexes = useMemo(() => {
    const query = filter.trim().toLowerCase()
    return entries
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) =>
        !query ||
        entry.wrong.toLowerCase().includes(query) ||
        entry.correct.toLowerCase().includes(query))
      .map(({ index }) => index)
  }, [entries, filter])

  const updateEntries = (next: TerminologyEntry[]) => {
    setEntries(next)
    setIsDirty(true)
  }

  const updateEntry = (index: number, patch: Partial<TerminologyEntry>) =>
    updateEntries(entries.map((entry, i) => (i === index ? { ...entry, ...patch } : entry)))

  const addDraft = () => {
    if (!draft.wrong.trim() || !draft.correct.trim()) return
    updateEntries([...entries, draft])
    setDraft(EMPTY_ENTRY)
  }

  const handleSave = async () => {
    setIsSaving(true)
    try {
      const saved = await saveTerminology(entries)
      setEntries(saved)
      setIsDirty(false)
      toast.success(`Saved ${saved.length} terminology ${saved.length === 1 ? "entry" : "entries"}`)
    } catch (error) {
      toast.error(`Failed to save terminology: ${error}`)
    } finally {
      setIsSaving(false)
    }
  }

  const handleImport = async (file: File | undefined) => {
    if (!file) return
    try {
      const imported = parseTerminologyImport(await file.text())
      updateEntries([...entries, ...imported])
      toast.success(`Imported ${imported.length} entries. Review and save to apply.`)
    } catch (error) {
      toast.error(`Import failed: ${error instanceof Error ? error.message : error}`)
    } finally {
      if (fileInputRef.current) fileInputRef.current.value = ""
    }
  }

  const handleExport = async () => {
    try {
      await navigator.clipboard.writeText(serializeTerminology(entries))
      toast.success("Terminology JSON copied to clipboard")
    } catch (error) {
      toast.error(`Failed to copy terminology: ${error}`)
    }
  }

  return (
    <div className="space-y-6">
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm space-y-4">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 mb-2">
              <BookA className="h-5 w-5 text-gray-600" />
              <h3 className="text-lg font-semibold text-gray-900">Terminology</h3>
            </div>
            <p className="text-sm text-gray-600">
              Words and phrases that are often transcribed incorrectly. Corrections are applied to every
              newly generated summary. Select text in a summary to add entries from there.
            </p>
          </div>
          <div className="flex gap-2 shrink-0">
            <input
              ref={fileInputRef}
              type="file"
              accept="application/json,.json"
              className="hidden"
              onChange={(e) => handleImport(e.target.files?.[0])}
            />
            <Button variant="outline" size="sm" onClick={() => fileInputRef.current?.click()}>
              <Upload /> Import
            </Button>
            <Button variant="outline" size="sm" onClick={handleExport} disabled={entries.length === 0}>
              <Copy /> Export
            </Button>
            <Button size="sm" onClick={handleSave} disabled={!isDirty || isSaving}>
              <Save /> {isSaving ? "Saving..." : "Save changes"}
            </Button>
          </div>
        </div>

        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
          <Input
            placeholder="Search terminology"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="pl-9"
          />
        </div>

        <div className="border border-gray-200 rounded-md overflow-hidden">
          <div className="grid grid-cols-[1fr_auto_1fr_auto_auto] items-center gap-3 px-4 py-2 bg-gray-50 text-xs font-medium uppercase text-gray-500">
            <span>Incorrect</span>
            <span />
            <span>Correct</span>
            <span>Match case</span>
            <span className="w-8" />
          </div>

          {visibleIndexes.map((index) => {
            const entry = entries[index]
            return (
              <div
                key={index}
                className="grid grid-cols-[1fr_auto_1fr_auto_auto] items-center gap-3 px-4 py-2 border-t border-gray-100"
              >
                <Input
                  value={entry.wrong}
                  onChange={(e) => updateEntry(index, { wrong: e.target.value })}
                  aria-label="Incorrect spelling"
                />
                <ArrowRight className="h-4 w-4 text-gray-400" />
                <Input
                  value={entry.correct}
                  onChange={(e) => updateEntry(index, { correct: e.target.value })}
                  aria-label="Correct spelling"
                />
                <div className="flex justify-center w-20">
                  <Switch
                    checked={entry.matchCase}
                    onCheckedChange={(checked) => updateEntry(index, { matchCase: checked })}
                  />
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  className="w-8 h-8 text-gray-500 hover:text-red-600"
                  onClick={() => updateEntries(entries.filter((_, i) => i !== index))}
                  aria-label="Delete entry"
                >
                  <Trash2 />
                </Button>
              </div>
            )
          })}

          {visibleIndexes.length === 0 && (
            <p className="px-4 py-6 text-sm text-center text-gray-500 border-t border-gray-100">
              {entries.length === 0 ? "No terminology entries yet." : "No entries match your search."}
            </p>
          )}

          <div className="grid grid-cols-[1fr_auto_1fr_auto_auto] items-center gap-3 px-4 py-3 border-t border-gray-200 bg-gray-50">
            <Input
              placeholder="e.g. post gres"
              value={draft.wrong}
              onChange={(e) => setDraft({ ...draft, wrong: e.target.value })}
              onKeyDown={(e) => e.key === "Enter" && addDraft()}
            />
            <ArrowRight className="h-4 w-4 text-gray-400" />
            <Input
              placeholder="e.g. Postgres"
              value={draft.correct}
              onChange={(e) => setDraft({ ...draft, correct: e.target.value })}
              onKeyDown={(e) => e.key === "Enter" && addDraft()}
            />
            <div className="flex justify-center w-20">
              <Switch
                checked={draft.matchCase}
                onCheckedChange={(checked) => setDraft({ ...draft, matchCase: checked })}
              />
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="w-8 h-8"
              onClick={addDraft}
              disabled={!draft.wrong.trim() || !draft.correct.trim()}
              aria-label="Add entry"
            >
              <Plus />
            </Button>
          </div>
        </div>
      </div>

      <div className="p-4 bg-blue-50 border border-blue-200 rounded-lg">
        <p className="text-sm text-blue-800">
          <strong>Note:</strong> Matching uses whole words only, so "JSAN" will not change "JSANX".
          Existing summaries are not changed; regenerate a summary to apply new entries.
        </p>
      </div>
    </div>
  )
}
