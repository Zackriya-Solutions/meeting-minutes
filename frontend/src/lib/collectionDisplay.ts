export interface CollectionDisplaySource {
  name: string;
  kind: 'manual' | 'series';
  system_key?: string | null;
}

type Translate = (source: string) => string;

export interface CollectionDisplayText {
  name: string;
  category: string;
  description: string;
}

/**
 * System collection names stored in the database are stable identifiers, not
 * user-facing copy. Resolve them by system_key so an existing collection
 * follows the current interface language without rewriting user data.
 */
export function getCollectionDisplayText(
  collection: CollectionDisplaySource,
  t: Translate,
): CollectionDisplayText {
  if (collection.system_key === 'inbox') {
    return {
      name: t('Memento Inbox'),
      category: t('Learning Inbox'),
      description: t(
        'Automatically captured meetings wait here until you review their type, people, terminology, and destination. Open a meeting to review it.',
      ),
    };
  }

  if (collection.kind === 'series') {
    return {
      name: collection.name,
      category: t('Recurring series'),
      description: t(
        'A recurring series combines repeated meetings and builds a cross-meeting digest. Automatic additions are controlled below.',
      ),
    };
  }

  return {
    name: collection.name,
    category: t('Manual collection'),
    description: t(
      'A manual collection contains only the meetings you select. Use it for a project, client, or topic, then search or ask questions within that scope.',
    ),
  };
}
