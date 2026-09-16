import type { Settings } from './types';

// Serialize writes so a slower save cannot overwrite a newer selection.
export class SettingsAutosave {
  private tail: Promise<void> = Promise.resolve();
  pending = 0;

  constructor(private write: (settings: Settings) => Promise<void>) {}

  save(settings: Settings): Promise<void> {
    this.pending++;
    this.tail = this.tail.catch(() => {}).then(() => this.write(settings)).finally(() => { this.pending--; });
    return this.tail;
  }
}
