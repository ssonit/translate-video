export function loadJoin(loaded: string, inflight: string, key: string): "ready" | "join" | "start" {
  if (loaded !== "" && loaded === key) return "ready";
  if (inflight !== "" && inflight === key) return "join";
  return "start";
}

export function acceptChunk(mine: number, current: number): boolean {
  return mine === current;
}

export function loadStillCurrent(started: number, now: number): boolean {
  return started === now;
}

// The next speak starts only after the one already running has returned, so two session.run calls do not overlap.
export function enqueueSpeak(tail: Promise<void>, job: () => Promise<unknown>): Promise<void> {
  return tail.then(job).then(
    () => undefined,
    () => undefined,
  );
}

export function slotAfterCrash(slot: { voice: string }): { alive: false; voice: string } {
  void slot;
  return { alive: false, voice: "" };
}
