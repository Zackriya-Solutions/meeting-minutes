'use client';

import { useEffect, useState, useCallback } from 'react';
import { Meeting } from '../../../types';

export default function MeetingsPage({ params }: { params: { id: string } }) {
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:5167';

  const fetchMeeting = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch(`${API_BASE_URL}/meetings/${params.id}`);
      if (!response.ok) {
        throw new Error('Failed to fetch meeting');
      }
      const data = await response.json();
      setMeeting(data);
    } catch (error) {
      console.error('Error fetching meeting:', error);
      setError('Failed to load meeting details');
    } finally {
      setLoading(false);
    }
  }, [API_BASE_URL, params.id]);

  useEffect(() => {
    fetchMeeting();
  }, [fetchMeeting]);

  const saveMeeting = useCallback(
    async (content: string) => {
      if (!meeting || meeting.content === content) return; // Avoid unnecessary API calls

      const updatedMeeting = { ...meeting, content };
      setMeeting(updatedMeeting);

      try {
        const response = await fetch(`${API_BASE_URL}/meetings/${params.id}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(updatedMeeting),
        });

        if (!response.ok) {
          throw new Error('Failed to save changes');
        }

        setToastMessage('Meeting saved successfully');
        setTimeout(() => setToastMessage(null), 3000);
      } catch (error) {
        console.error('Error saving meeting:', error);
        setToastMessage('Failed to save changes');
      }
    },
    [meeting, API_BASE_URL, params.id]
  );

  const handleContentChange = (newContent: string) => {
    debounce(() => saveMeeting(newContent), 500)();
  };

  const handleManualSave = () => {
    if (meeting) saveMeeting(meeting.content);
  };

  const handleDelete = async () => {
    try {
      const response = await fetch(`${API_BASE_URL}/meetings/${params.id}`, {
        method: 'DELETE',
      });
      if (!response.ok) {
        throw new Error('Failed to delete meeting');
      }
      window.location.href = '/meetings'; // Redirect after deletion
    } catch (error) {
      console.error('Error deleting meeting:', error);
      setError('Failed to delete meeting');
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <div className="text-xl font-semibold animate-pulse">Loading meeting details...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="max-w-md mx-auto my-10 p-6 bg-red-50 border border-red-200 rounded-md shadow">
        <h3 className="text-lg font-bold text-red-800">Error</h3>
        <p className="mt-2 text-red-600">{error}</p>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-50 relative">
      {toastMessage && (
        <div className="fixed top-5 right-5 bg-green-600 text-white px-4 py-2 rounded shadow">
          {toastMessage}
        </div>
      )}

      <header className="bg-gradient-to-r from-indigo-600 to-blue-500 py-10">
        <div className="container mx-auto px-4 text-center">
          <h1 className="text-4xl font-extrabold text-white">{meeting?.title || 'Meeting Details'}</h1>
          <p className="mt-2 text-lg text-indigo-200">Manage and review your meeting details seamlessly</p>
        </div>
      </header>

      <main className="container mx-auto px-4 py-10">
        <div className="bg-white rounded-lg shadow-lg p-6">
          <div className="mb-6">
            <label htmlFor="meetingContent" className="block text-sm font-medium text-gray-700">
              Meeting Notes
            </label>
            <textarea
              id="meetingContent"
              value={meeting?.content || ''}
              onChange={(e) => handleContentChange(e.target.value)}
              rows={10}
              className="mt-1 block w-full px-4 py-3 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-indigo-500 focus:border-indigo-500 resize-none"
              placeholder="Enter meeting notes..."
            />
            <div className="mt-4">
              <button
                type="button"
                onClick={handleManualSave}
                className="px-4 py-2 bg-green-600 text-white font-semibold rounded-md shadow hover:bg-green-500 focus:outline-none focus:ring-2 focus:ring-green-600 focus:ring-opacity-50"
              >
                Save
              </button>
            </div>
          </div>

          <div className="mb-6 border-t pt-6">
            <h2 className="text-xl font-semibold text-gray-800 mb-3">Preview</h2>
            <div className="p-4 bg-gray-100 rounded-md">
              {meeting?.content ? (
                <p className="text-gray-700 leading-relaxed whitespace-pre-line">{meeting.content}</p>
              ) : (
                <p className="text-gray-500 italic">No content available yet.</p>
              )}
            </div>
          </div>

          <div className="flex justify-end space-x-4 border-t pt-6">
            <button
              type="button"
              onClick={handleDelete}
              className="px-6 py-2 bg-red-600 text-white font-semibold rounded-md shadow hover:bg-red-500 focus:outline-none focus:ring-2 focus:ring-red-600 focus:ring-opacity-50"
            >
              Delete Meeting
            </button>
          </div>
        </div>
      </main>
    </div>
  );
}

// Debounce function to prevent excessive API calls
const debounce = (func: Function, delay: number) => {
  let timer: NodeJS.Timeout;
  return (...args: any[]) => {
    clearTimeout(timer);
    timer = setTimeout(() => func(...args), delay);
  };
};
