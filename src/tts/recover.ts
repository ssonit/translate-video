class SpeechStalled extends Error {
  constructor() { super("Bộ đọc giọng không trả lời cho câu này."); }
}

async function bounded<T>(read: () => Promise<T>, milliseconds: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      read(),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new SpeechStalled()), milliseconds); }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// Only retry a stalled short sentence, once. reset must terminate the old worker
// before loading its replacement so it cannot deliver an old PCM to the new run.
export async function recoverSpeech<T>(
  read: () => Promise<T>,
  reset: () => Promise<void>,
  cancelled: () => boolean,
  milliseconds = 45_000,
): Promise<T> {
  try {
    return await bounded(read, milliseconds);
  } catch (error) {
    if (!(error instanceof SpeechStalled) || cancelled()) throw error;
    await reset();
    if (cancelled()) throw new Error("Đã dừng.");
    return bounded(read, milliseconds);
  }
}
