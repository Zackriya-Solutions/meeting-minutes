'use client';

import { useEffect, useRef, useState } from 'react';
import { Check, Plus, Sparkles, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useSidebar, UNFILED_FOLDER_VALUE } from '@/components/Sidebar/SidebarProvider';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

interface OrganizationTag { id: string; name: string; }

interface MeetingOrganizationPanelProps {
  meetingId: string;
  folderId?: string | null;
  tags?: OrganizationTag[];
  hasContent: boolean;
}

interface StoredTagSuggestions {
  status: 'generated' | 'dismissed';
  suggestions: string[];
}

const EMPTY_TAGS: OrganizationTag[] = [];

function suggestionsStorageKey(meetingId: string): string {
  return `meetingTagSuggestions:${meetingId}`;
}

function readStoredSuggestions(meetingId: string): StoredTagSuggestions | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(suggestionsStorageKey(meetingId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as StoredTagSuggestions;
    if (parsed.status !== 'generated' && parsed.status !== 'dismissed') return null;
    return {
      status: parsed.status,
      suggestions: Array.isArray(parsed.suggestions) ? parsed.suggestions.filter(item => typeof item === 'string') : [],
    };
  } catch {
    return null;
  }
}

function writeStoredSuggestions(meetingId: string, value: StoredTagSuggestions): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(suggestionsStorageKey(meetingId), JSON.stringify(value));
  } catch {
    // Suggestion caching is best-effort; manual tags are unaffected.
  }
}

export function MeetingOrganizationPanel({ meetingId, folderId, tags: initialTags = EMPTY_TAGS, hasContent }: MeetingOrganizationPanelProps) {
  const { projectFolders, meetings, refetchOrganization } = useSidebar();
  const [tags, setTags] = useState<OrganizationTag[]>(initialTags);
  const [newTag, setNewTag] = useState('');
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [suggestionsDismissed, setSuggestionsDismissed] = useState(false);
  const [isSuggesting, setIsSuggesting] = useState(false);
  const [noSuggestionsFound, setNoSuggestionsFound] = useState(false);
  const suggestionTokenRef = useRef(0);

  const sidebarMeeting = meetings.find(meeting => meeting.id === meetingId);
  const currentFolderId = sidebarMeeting ? sidebarMeeting.project_folder_id ?? null : folderId ?? null;

  useEffect(() => setTags(initialTags), [initialTags]);

  useEffect(() => {
    suggestionTokenRef.current += 1;
    const stored = readStoredSuggestions(meetingId);
    setSuggestions(stored?.status === 'generated' ? stored.suggestions : []);
    setSuggestionsDismissed(stored?.status === 'dismissed');
    setNoSuggestionsFound(false);
    setIsSuggesting(false);
  }, [meetingId]);

  useEffect(() => {
    if (!hasContent || readStoredSuggestions(meetingId) !== null) return;
    writeStoredSuggestions(meetingId, { status: 'generated', suggestions: [] });
    const token = suggestionTokenRef.current;
    void invoke('api_suggest_meeting_tags', { meetingId }).then((result) => {
      if (suggestionTokenRef.current !== token) return;
      const generated = result as string[];
      writeStoredSuggestions(meetingId, { status: 'generated', suggestions: generated });
      setSuggestions(generated);
    }).catch(() => { /* Optional enhancement: manual tags still work. */ });
    return () => { suggestionTokenRef.current += 1; };
  }, [meetingId, hasContent]);

  const updateSuggestions = (next: string[], dismissed: boolean) => {
    if (dismissed) suggestionTokenRef.current += 1;
    setSuggestions(next);
    setSuggestionsDismissed(dismissed);
    setNoSuggestionsFound(false);
    writeStoredSuggestions(meetingId, { status: dismissed ? 'dismissed' : 'generated', suggestions: dismissed ? [] : next });
  };

  const requestSuggestions = async () => {
    suggestionTokenRef.current += 1;
    const token = suggestionTokenRef.current;
    setIsSuggesting(true);
    try {
      const generated = await invoke('api_suggest_meeting_tags', { meetingId }) as string[];
      if (suggestionTokenRef.current !== token) return;
      updateSuggestions(generated, false);
      setNoSuggestionsFound(generated.length === 0);
    } catch {
      // No model configured or the call failed: stay manual and quiet.
    } finally {
      setIsSuggesting(false);
    }
  };

  const addTag = async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed || tags.some(tag => tag.name.toLowerCase() === trimmed.toLowerCase())) return;
    try {
      const tag = await invoke('api_add_meeting_tag', { meetingId, name: trimmed }) as OrganizationTag;
      setTags(previous => [...previous, tag].sort((left, right) => left.name.localeCompare(right.name)));
      setNewTag('');
      if (!suggestionsDismissed) {
        updateSuggestions(suggestions.filter(suggestion => suggestion.toLowerCase() !== trimmed.toLowerCase()), false);
      }
      await refetchOrganization();
    } catch (error) { toast.error('Could not add tag', { description: error instanceof Error ? error.message : String(error) }); }
  };

  const removeTag = async (tag: OrganizationTag) => {
    try {
      await invoke('api_remove_meeting_tag', { meetingId, tagId: tag.id });
      setTags(previous => previous.filter(current => current.id !== tag.id));
      await refetchOrganization();
    } catch (error) { toast.error('Could not remove tag', { description: error instanceof Error ? error.message : String(error) }); }
  };

  const moveMeeting = async (value: string) => {
    const nextFolderId = value === UNFILED_FOLDER_VALUE ? null : value;
    try {
      await invoke('api_move_meeting_to_project_folder', { meetingId, folderId: nextFolderId });
      await refetchOrganization();
    } catch (error) { toast.error('Could not move meeting', { description: error instanceof Error ? error.message : String(error) }); }
  };

  const visibleSuggestions = suggestions.filter(suggestion => !tags.some(tag => tag.name.toLowerCase() === suggestion.toLowerCase()));

  return (
    <div className="mt-3 rounded-lg border border-gray-200 bg-gray-50 p-3 text-left">
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor="meeting-project-folder" className="text-xs font-medium text-gray-600">Project</label>
        <Select value={currentFolderId ?? UNFILED_FOLDER_VALUE} onValueChange={(value) => void moveMeeting(value)}>
          <SelectTrigger id="meeting-project-folder" className="h-8 w-40 bg-white text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={UNFILED_FOLDER_VALUE} className="text-xs">Unfiled</SelectItem>
            {projectFolders.map(folder => <SelectItem key={folder.id} value={folder.id} className="text-xs">{folder.name}</SelectItem>)}
          </SelectContent>
        </Select>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        {tags.map(tag => <span key={tag.id} className="inline-flex items-center gap-1 rounded-full bg-blue-100 px-2 py-1 text-xs text-blue-700">#{tag.name}<button aria-label={`Remove tag ${tag.name}`} onClick={() => void removeTag(tag)}><X className="h-3 w-3" /></button></span>)}
        <form onSubmit={(event) => { event.preventDefault(); void addTag(newTag); }} className="inline-flex items-center gap-1">
          <Input aria-label="Add meeting tag" value={newTag} onChange={(event) => setNewTag(event.target.value)} placeholder="Add tag" className="h-8 w-24 bg-white text-xs" />
          <Button type="submit" variant="ghost" size="icon" aria-label="Add tag" className="h-8 w-8 text-gray-500 hover:text-blue-600">
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </form>
        {hasContent && (
          <Button variant="outline" size="sm" onClick={() => void requestSuggestions()} disabled={isSuggesting} className="h-8 bg-white text-xs text-gray-600 hover:text-blue-600">
            <Sparkles className="h-3 w-3" />{isSuggesting ? 'Suggesting...' : 'Suggest tags'}
          </Button>
        )}
      </div>
      {noSuggestionsFound && !isSuggesting && (
        <p className="mt-2 text-xs text-gray-500">No tag suggestions found for this meeting.</p>
      )}
      {!suggestionsDismissed && visibleSuggestions.length > 0 && (
        <div className="mt-2 border-t border-gray-200 pt-2">
          <div className="flex items-center justify-between text-xs text-gray-500">
            <span>Suggested tags</span>
            <Button variant="ghost" size="sm" onClick={() => updateSuggestions([], true)} className="h-6 px-2 text-xs text-gray-500 hover:text-gray-800">Discard</Button>
          </div>
          <div className="mt-1 flex flex-wrap gap-1.5">{visibleSuggestions.map(suggestion => <span key={suggestion} className="inline-flex items-center gap-1 rounded-full border border-dashed border-blue-300 bg-white px-2 py-1 text-xs text-blue-700"><button onClick={() => void addTag(suggestion)} className="inline-flex items-center gap-1 hover:text-blue-900"><Check className="h-3 w-3" />{suggestion}</button><button onClick={() => setNewTag(suggestion)} className="border-l border-blue-200 pl-1 text-[10px] hover:text-blue-900">Edit</button></span>)}</div>
        </div>
      )}
    </div>
  );
}
