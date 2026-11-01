/** A duration, as a job's progress says it: "42s", "3m 5s". */
export function Elapsed({ seconds }: { seconds: number }) {
  return <>{seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`}</>;
}
