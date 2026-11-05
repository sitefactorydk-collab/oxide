import { useEffect, useState } from "react";

/** `value`, updated only after it has stopped changing for `ms`. Empty values apply at once. */
export function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    if (value === "" || ms <= 0) {
      setDebounced(value);
      return;
    }
    const t = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return debounced;
}
