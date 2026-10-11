'use client';

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

export function SpeakerCountSelect({ value, onChange }: { value: number | null; onChange: (value: number | null) => void }) {
  return (
    <Select value={value === null ? 'auto' : String(value)} onValueChange={(v) => onChange(v === 'auto' ? null : Number(v))}>
      <SelectTrigger className="w-32"><SelectValue /></SelectTrigger>
      <SelectContent>
        <SelectItem value="auto">Auto</SelectItem>
        {[2, 3, 4, 5, 6, 7, 8, 9, 10].map((n) => (
          <SelectItem key={n} value={String(n)}>{n} speakers</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
