'use client';

import { useRouter } from 'next/navigation';
import { Loader2 } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import { Icon } from '@/components/memento/Icon';
import { useKnowledgeReadiness } from '@/hooks/useKnowledgeReadiness';
import { useT } from '@/lib/i18n';

export function KnowledgeReadinessCard({ mode }: { mode: 'search' | 'chat' }) {
  const t = useT();
  const router = useRouter();
  const readiness = useKnowledgeReadiness();
  const noSearchableMeetings = !readiness.loading && readiness.searchableMeetings === 0;
  const chatBlocked = mode === 'chat' && !readiness.loading && !readiness.chatAllowed;

  return (
    <section className="mx-auto mb-5 w-full max-w-3xl rounded-2xl border border-border bg-background p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Icon name={mode === 'search' ? 'search' : 'library'} size={18} className="text-primary" />
            <h2 className="text-sm font-semibold text-foreground">
              {mode === 'search' ? t('How meeting search works') : t('How the knowledge base works')}
            </h2>
          </div>
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
            {mode === 'search'
              ? t('Enter words or a natural-language phrase. Memento returns matching transcript fragments; open a result to jump to that moment in the meeting.')
              : t('Choose the whole archive, a collection, or one meeting. Memento finds relevant fragments locally, sends only those fragments with your question to DeepSeek, and returns an answer with source links.')}
          </p>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => router.push(mode === 'chat' ? '/settings?tab=privacy' : '/settings?tab=search')}
        >
          <Icon name="settings" size={15} />
          {mode === 'chat' ? t('Privacy settings') : t('Search settings')}
        </Button>
      </div>

      <div className="mt-3 flex flex-wrap gap-2 text-xs">
        {readiness.loading ? (
          <span className="flex items-center gap-1.5 text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('Checking archive index…')}
          </span>
        ) : (
          <>
            <StatusPill
              ok={readiness.searchableMeetings > 0}
              text={`${readiness.searchableMeetings}/${readiness.indexableMeetings} ${t('meetings indexed')}`}
            />
            <StatusPill
              ok={readiness.semanticEnabled}
              text={
                readiness.semanticEnabled
                  ? `${t('Semantic search')}: ${readiness.semanticCoverage}%`
                  : t('Keyword search only')
              }
            />
            {readiness.indexingActive && (
              <span className="rounded-full bg-primary/10 px-2.5 py-1 text-primary">
                {t('Indexing in progress')}
              </span>
            )}
            {mode === 'chat' && (
              <StatusPill
                ok={readiness.chatAllowed}
                text={readiness.chatAllowed ? t('AI answers allowed') : t('AI answers blocked by privacy settings')}
              />
            )}
          </>
        )}
      </div>

      {noSearchableMeetings && (
        <p className="mt-3 rounded-lg bg-primary/10 px-3 py-2 text-xs text-primary">
          {t('No meetings have been indexed yet. Record or import a meeting, then use “Check and repair index” in Settings → Search. A summary is not required.')}
        </p>
      )}
      {chatBlocked && (
        <p className="mt-3 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {readiness.localOnly
            ? t('Local-only mode blocks the managed DeepSeek chat. Turn it off in Settings → Privacy to use the knowledge base.')
            : t('Archive questions are disabled in Settings → Privacy.')}
        </p>
      )}
      {readiness.error && (
        <p className="mt-3 text-xs text-destructive">{t('Archive index status unavailable')}</p>
      )}
    </section>
  );
}

function StatusPill({ ok, text }: { ok: boolean; text: string }) {
  return (
    <span
      className={
        ok
          ? 'rounded-full bg-success/10 px-2.5 py-1 text-success'
          : 'rounded-full bg-muted px-2.5 py-1 text-muted-foreground'
      }
    >
      {text}
    </span>
  );
}
