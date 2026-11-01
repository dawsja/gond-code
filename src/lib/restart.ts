/**
 * What has to be saved before the app restarts into an update, so it opens exactly where you
 * left off: the sessions and their panes, and each terminal's screen. Each part registers how to
 * save itself; Restart to update waits for all of them (a few seconds at most).
 */
const savers = new Set<() => Promise<unknown> | unknown>();

export function onBeforeRestart(save: () => Promise<unknown> | unknown): () => void {
  savers.add(save);
  return () => void savers.delete(save);
}

export async function prepareRestart(timeoutMs = 4000) {
  const all = Promise.allSettled([...savers].map((save) => Promise.resolve().then(save)));
  await Promise.race([all, new Promise((done) => setTimeout(done, timeoutMs))]);
}
