import { useEffect, useRef } from "react";

/**
 * What an empty session shows for the moment before its shell starts: nothing; it asks once
 * for a terminal in the default shell.
 */
export function AutoShell({ onEmpty }: { onEmpty: () => void }) {
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    onEmpty();
  }, [onEmpty]);
  return null;
}
