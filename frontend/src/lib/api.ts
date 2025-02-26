const API_BASE = 'http://localhost:5167';

export interface Meeting {
  id: string;
  title: string;
  date: string;
  time?: string;
  attendees: string[];
  tags: string[];
  content: string;
  created_at: string;
  updated_at: string;
}

export async function getMeetings(): Promise<Meeting[]> {
  const response = await fetch(`${API_BASE}/meetings`);
  if (!response.ok) {
    throw new Error('Failed to fetch meetings');
  }
  return response.json();
}

export async function getMeeting(id: string): Promise<Meeting> {
  const response = await fetch(`${API_BASE}/meetings/${id}`);
  if (!response.ok) {
    throw new Error('Failed to fetch meeting');
  }
  return response.json();
}

export async function deleteMeeting(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/meetings/${id}`, {
    method: 'DELETE',
  });
  if (!response.ok) {
    throw new Error('Failed to delete meeting');
  }
} 