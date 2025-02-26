'use client';

import React, { useEffect, useState } from 'react';
import { Clock, Users, Calendar, Tag, Trash2 } from 'lucide-react';
import { getMeeting, deleteMeeting, type Meeting } from '@/lib/api';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
interface MeetingContentProps {
  meetingId: string;
}

export default function MeetingContent({ meetingId }: MeetingContentProps) {
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const router = useRouter();

  useEffect(() => {
    const fetchMeeting = async () => {
      try {
        const data = await getMeeting(meetingId);
        setMeeting(data);
      } catch (err) {
        setError('Failed to load meeting');
      } finally {
        setLoading(false);
      }
    };
    fetchMeeting();
  }, [meetingId]);

  const handleDelete = async () => {
    try {
      setIsDeleting(true);
      await deleteMeeting(meetingId);
      router.push('/meetings');
    } catch (err) {
      setError('Failed to delete meeting');
    } finally {
      setIsDeleting(false);
    }
  };

  if (loading) return <div className="p-8">Loading...</div>;
  if (error) return <div className="p-8 text-red-500">{error}</div>;
  if (!meeting) return <div className="p-8">Meeting not found</div>;

  return (
    <div className="p-8 max-w-4xl mx-auto">
      <div className="mb-8">
        <div className="flex justify-between items-start">
          <h1 className="text-3xl font-bold mb-4">{meeting.title}</h1>
          <Button
            variant="destructive"
            size="sm"
            onClick={() => setShowDeleteDialog(true)}
          >
            <Trash2 className="w-4 h-4 mr-2" />
            Delete
          </Button>
        </div>

        <div className="space-y-4">
          <div className="flex items-center text-gray-600">
            <Calendar className="w-4 h-4 mr-2" />
            <span>{meeting.date}</span>
            {meeting.time && (
              <>
                <Clock className="w-4 h-4 ml-4 mr-2" />
                <span>{meeting.time}</span>
              </>
            )}
          </div>

          {meeting.attendees.length > 0 && (
            <div className="flex items-center text-gray-600">
              <Users className="w-4 h-4 mr-2" />
              <span>{meeting.attendees.join(', ')}</span>
            </div>
          )}

          {meeting.tags.length > 0 && (
            <div className="flex items-center text-gray-600">
              <Tag className="w-4 h-4 mr-2" />
              <div className="flex gap-2">
                {meeting.tags.map((tag) => (
                  <span
                    key={tag}
                    className="bg-gray-100 px-2 py-1 rounded-full text-sm"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="mt-8 prose max-w-none">
          <div dangerouslySetInnerHTML={{ __html: meeting.content }} />
        </div>
      </div>

      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Are you sure?</AlertDialogTitle>
            <AlertDialogDescription>
              This action cannot be undone. This will permanently delete the meeting
              and its contents.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>
              {isDeleting ? 'Deleting...' : 'Delete'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}