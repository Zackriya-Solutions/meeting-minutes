/** Runs `run` with `console.error` silenced, restoring it afterwards. */
export async function withQuietConsoleError<T>(run: () => Promise<T>): Promise<T> {
  const original = console.error;
  console.error = () => {};
  try {
    return await run();
  } finally {
    console.error = original;
  }
}
