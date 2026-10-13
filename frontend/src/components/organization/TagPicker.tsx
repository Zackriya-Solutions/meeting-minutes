'use client';

import { useState } from 'react';
import { Check, Plus, Tag as TagIcon } from 'lucide-react';
import { toast } from 'sonner';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
import type { Tag } from '@/lib/organization';

interface TagPickerProps {
  tags: Tag[];
  selectedIds: string[];
  onChange: (tagIds: string[]) => void;
  onCreate: (name: string) => Promise<{ id: string }>;
}

export function TagPicker({ tags, selectedIds, onChange, onCreate }: TagPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const trimmed = query.trim();
  const exactMatch = tags.some(t => t.name.toLowerCase() === trimmed.toLowerCase());
  const selectedNames = tags.filter(t => selectedIds.includes(t.id)).map(t => `#${t.name}`);

  const toggle = (id: string) =>
    onChange(selectedIds.includes(id) ? selectedIds.filter(x => x !== id) : [...selectedIds, id]);

  const create = async () => {
    if (!trimmed) return;
    try {
      const tag = await onCreate(trimmed);
      if (!selectedIds.includes(tag.id)) onChange([...selectedIds, tag.id]);
      setQuery('');
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="inline-flex items-center gap-1 rounded-md border border-gray-200 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-50"
        >
          <TagIcon className="h-3 w-3" />
          {selectedNames.length ? selectedNames.join(' ') : '+ tags'}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-64 p-0" align="start">
        <Command>
          <CommandInput placeholder="Find or create a tag…" value={query} onValueChange={setQuery} />
          <CommandList>
            <CommandEmpty>No tags yet</CommandEmpty>
            <CommandGroup>
              {tags.map(tag => (
                <CommandItem key={tag.id} value={tag.name} onSelect={() => toggle(tag.id)}>
                  <Check className={`mr-2 h-3 w-3 ${selectedIds.includes(tag.id) ? 'opacity-100' : 'opacity-0'}`} />
                  #{tag.name}
                </CommandItem>
              ))}
              {trimmed && !exactMatch && (
                <CommandItem value={`create ${trimmed}`} onSelect={() => void create()}>
                  <Plus className="mr-2 h-3 w-3" />
                  Create “{trimmed}”
                </CommandItem>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
