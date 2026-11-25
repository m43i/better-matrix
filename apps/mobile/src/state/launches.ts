// Launches the app itself asked for: screens that must not start from a link alone (the widget
// screen starting a meeting or opening the integration manager) take a one-time nonce, which
// the in-app action registers just before it opens the screen. Kept in memory for the run of
// the app; each is used once.

export class Launches {
  private readonly open = new Set<string>();

  constructor(
    /** A fresh, unguessable nonce. */
    private readonly nonce: () => string,
  ) {}

  /** Registers a launch; its nonce goes in the screen's route. */
  register(): string {
    const nonce = this.nonce();
    this.open.add(nonce);

    return nonce;
  }

  /** Whether `nonce` is a launch the app registered and hasn't used yet; it is used up. */
  consume(nonce: string | undefined): boolean {
    return nonce !== undefined && this.open.delete(nonce);
  }
}
