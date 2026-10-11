type Listener = () => void;

/** Every FakeAudio created, newest last. */
export const audios: FakeAudio[] = [];

/** Just enough of HTMLAudioElement: tests set currentTime/error and emit the events. */
export class FakeAudio {
  static canPlay = 'maybe';
  src = '';
  currentTime = 0;
  playbackRate = 1;
  defaultPlaybackRate = 1;
  paused = true;
  ended = false;
  preload = '';
  error: { code: number } | null = null;
  private listeners = new Map<string, Set<Listener>>();
  constructor() { audios.push(this); }
  canPlayType() { return FakeAudio.canPlay; }
  addEventListener(type: string, listener: Listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type)!.add(listener);
  }
  removeEventListener(type: string, listener: Listener) { this.listeners.get(type)?.delete(listener); }
  removeAttribute(name: string) { if (name === 'src') this.src = ''; }
  load() {}
  play() {
    this.paused = false;
    this.ended = false;
    this.emit('play');
    return Promise.resolve();
  }
  pause() {
    if (this.paused) return;
    this.paused = true;
    this.emit('pause');
  }
  emit(type: string) { for (const listener of [...(this.listeners.get(type) ?? [])]) listener(); }
}
