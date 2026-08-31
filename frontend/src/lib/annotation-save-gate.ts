export class AnnotationSaveGate {
  private locked = false;
  private writes = new Set<Promise<void>>();

  begin(): void {
    this.locked = true;
  }

  finish(): void {
    this.locked = false;
  }

  canWrite(): boolean {
    return !this.locked;
  }

  track(write: Promise<void>): Promise<void> {
    this.writes.add(write);
    write.then(
      () => this.writes.delete(write),
      () => this.writes.delete(write),
    );
    return write;
  }

  async wait(): Promise<void> {
    while (this.writes.size > 0) {
      await Promise.all(Array.from(this.writes));
    }
  }
}
