import { RefObject, useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';
import { summaryToMarkdown } from '@/lib/summaryToMarkdown';
import { buildTelegramShareParts, TELEGRAM_MESSAGE_LIMIT } from '@/lib/telegramShare';
import { Summary } from '@/types';

/** `app_settings_kv` key: open the Telegram picker as soon as a summary finishes. */
export const TELEGRAM_AUTO_SHARE_KEY = 'telegram.auto_share';

interface UseTelegramShareProps {
  meeting: { id: string; created_at?: string } | null;
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
}

/**
 * Sharing the current meeting's summary to Telegram.
 *
 * Telegram is handed the text through its share deep link, so this opens the chat picker
 * and stops there — the user picks the chat and presses send. Summaries too long for one
 * message are written to a `.md` file and revealed in the file manager, because a deep
 * link has no way to carry an attachment.
 */
export function useTelegramShare({
  meeting,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
}: UseTelegramShareProps) {
  const t = useT();
  const [localOnly, setLocalOnly] = useState(false);
  const [isSharing, setIsSharing] = useState(false);
  const autoSharedRef = useRef<Set<string>>(new Set());

  // Local-only mode means nothing leaves this machine, and Telegram is another machine.
  useEffect(() => {
    let cancelled = false;
    invoke<Record<string, string>>('get_app_settings')
      .then((settings) => {
        if (!cancelled) {
          setLocalOnly((settings?.['privacy.local_only'] || '').trim().toLowerCase() === 'true');
        }
      })
      .catch(() => {
        // Leave sharing available; the Rust command re-checks and fails closed.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * For a manual share, the editor is the source of truth — it may hold unsaved edits.
   * A caller-supplied summary overrides it: that path runs the moment generation
   * finishes, when the editor is still showing the *previous* summary.
   */
  const resolveMarkdown = useCallback(
    async (summary: Summary | null, preferSummary: boolean): Promise<string> => {
      if (!preferSummary && blockNoteSummaryRef.current?.getMarkdown) {
        try {
          const fromEditor = await blockNoteSummaryRef.current.getMarkdown();
          if (fromEditor?.trim()) return fromEditor;
        } catch (error) {
          console.warn('Telegram share: editor markdown unavailable, using saved summary', error);
        }
      }
      return summaryToMarkdown(summary);
    },
    [blockNoteSummaryRef],
  );

  const shareSummary = useCallback(
    async (summaryOverride?: Summary | null) => {
      if (!meeting?.id) return;

      const hasOverride = summaryOverride !== undefined;
      const summary = hasOverride ? summaryOverride : aiSummary;
      setIsSharing(true);
      try {
        const markdown = await resolveMarkdown(summary ?? null, hasOverride);
        if (!markdown.trim()) {
          toast.error(t('No summary content available to share'));
          return;
        }

        const { draft, body, full } = buildTelegramShareParts({
          title: meetingTitle,
          createdAt: meeting.created_at,
          markdown,
          draftPlaceholder: t('<paste the text from the clipboard>'),
        });

        // The body goes on the clipboard: the deep link cannot carry it without being
        // truncated or corrupted, and the clipboard has no size limit of its own.
        let copied = true;
        try {
          await navigator.clipboard.writeText(body || full);
        } catch (error) {
          // Auto-share runs without a user gesture, which the webview may refuse to
          // service. The share still proceeds — the draft and the file are unaffected.
          copied = false;
          console.warn('Telegram share: clipboard write refused', error);
        }

        // Past one message the paste cannot go through as a single message either, so also
        // leave a file the user can drag in.
        let filePath: string | null = null;
        if (full.length > TELEGRAM_MESSAGE_LIMIT) {
          filePath = await invoke<string>('save_summary_markdown_file', {
            meetingId: meeting.id,
            markdown: full,
          });
          // Reveal first so Telegram's picker ends up on top of the file manager.
          await invoke('reveal_report_in_folder', { path: filePath }).catch((error) => {
            console.warn('Telegram share: could not reveal the summary file', error);
          });
        }

        await invoke('telegram_share_text', { text: draft });

        if (filePath) {
          toast.info(t('Summary is longer than one Telegram message'), {
            description: t('Pick a chat, then drag in the file revealed in the folder. The text is also on the clipboard.'),
            duration: 9000,
          });
        } else if (copied) {
          toast.success(t('Telegram opened — pick a chat and paste the summary'), {
            description: t('Replace the placeholder line in the draft with the clipboard contents. The link on the first line is required by Telegram; delete it if you do not need it.'),
            duration: 9000,
          });
        } else {
          toast.warning(t('Telegram opened, but the summary could not be copied'), {
            description: t('Use the Copy action and paste it into the chat manually.'),
            duration: 9000,
          });
        }

        await Analytics.trackButtonClick('share_summary_telegram', 'meeting_details');
      } catch (error) {
        console.error('Failed to share summary to Telegram:', error);
        toast.error(typeof error === 'string' ? error : t('Failed to open Telegram'));
      } finally {
        setIsSharing(false);
      }
    },
    [aiSummary, meeting?.created_at, meeting?.id, meetingTitle, resolveMarkdown, t],
  );

  /**
   * Called when a generation run finishes. Re-reads the preference rather than trusting
   * mount-time state, so a toggle flipped mid-meeting takes effect immediately.
   */
  const autoShareIfEnabled = useCallback(
    async (summary: Summary | null) => {
      const meetingId = meeting?.id;
      if (!meetingId || autoSharedRef.current.has(meetingId)) return;

      try {
        const settings = await invoke<Record<string, string>>('get_app_settings');
        const enabled = (settings?.[TELEGRAM_AUTO_SHARE_KEY] || '').trim().toLowerCase() === 'true';
        const blocked = (settings?.['privacy.local_only'] || '').trim().toLowerCase() === 'true';
        if (!enabled || blocked) return;
      } catch (error) {
        console.warn('Telegram auto-share: settings unavailable, skipping', error);
        return;
      }

      // Once per meeting per session: a regenerate should not reopen Telegram behind the
      // user's back after they have already dealt with this summary.
      autoSharedRef.current.add(meetingId);
      await shareSummary(summary);
    },
    [meeting?.id, shareSummary],
  );

  return {
    /** False when the user has asked for local-only operation. */
    canShareToTelegram: !localOnly,
    isSharing,
    shareSummary,
    autoShareIfEnabled,
  };
}
