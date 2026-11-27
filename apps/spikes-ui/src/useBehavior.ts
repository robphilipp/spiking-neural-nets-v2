import { useEffect, useState } from "react";
import { type BehaviorSubject, throttleTime } from "rxjs";

/** A behavior subject's current value, following it (null without one); with `everyMs`, at most that often. */
export function useBehavior<T>(subject: BehaviorSubject<T> | null | undefined, everyMs = 0): T | null {
  const [value, setValue] = useState<T | null>(subject?.value ?? null);
  useEffect(() => {
    if (!subject) return setValue(null);
    // Throttled: at most one update per `everyMs`, the first at once and the latest at the end of each interval (so
    // the final value, when a run stops, always shows).
    const values = everyMs > 0 ? subject.pipe(throttleTime(everyMs, undefined, { leading: true, trailing: true })) : subject;
    const subscription = values.subscribe({ next: setValue, complete: () => setValue(null) });
    return () => subscription.unsubscribe();
  }, [subject, everyMs]);
  return value;
}
