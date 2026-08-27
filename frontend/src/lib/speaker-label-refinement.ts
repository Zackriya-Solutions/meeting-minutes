let pendingRefinement: Promise<void> | null = null;

export function trackSpeakerLabelRefinement(work: Promise<void>): void {
  const tracked = work
    .catch(error => {
      console.warn('Failed to apply refined speaker labels:', error);
    })
    .finally(() => {
      if (pendingRefinement === tracked) {
        pendingRefinement = null;
      }
    });
  pendingRefinement = tracked;
}

export async function waitForSpeakerLabelRefinement(): Promise<void> {
  while (pendingRefinement) {
    await pendingRefinement;
  }
}
