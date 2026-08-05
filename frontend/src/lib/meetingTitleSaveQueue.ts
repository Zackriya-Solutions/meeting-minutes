export function normalizeMeetingTitle(title: string): string {
  return title.trim();
}

export function isLatestMeetingTitle(currentTitle: string, completedTitle: string): boolean {
  return normalizeMeetingTitle(currentTitle) === completedTitle;
}

/** Serializes title writes and shares one promise between duplicate callers. */
export class MeetingTitleSaveQueue {
  private tail: Promise<void> = Promise.resolve();
  private readonly inFlight = new Map<string, Promise<void>>();

  enqueue(title: string, save: () => Promise<void>): Promise<void> {
    const existing = this.inFlight.get(title);
    if (existing) return existing;

    const request = this.tail.then(save);
    this.tail = request.catch(() => undefined);
    this.inFlight.set(title, request);

    const cleanup = () => {
      if (this.inFlight.get(title) === request) this.inFlight.delete(title);
    };
    void request.then(cleanup, cleanup);
    return request;
  }
}
