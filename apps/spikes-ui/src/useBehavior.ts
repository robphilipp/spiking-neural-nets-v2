import { useEffect, useState } from "react";
import type { BehaviorSubject } from "rxjs";

/** A behavior subject's current value, following it (null without one). */
export function useBehavior<T>(subject: BehaviorSubject<T> | null | undefined): T | null {
  const [value, setValue] = useState<T | null>(subject?.value ?? null);
  useEffect(() => {
    if (!subject) return setValue(null);
    const subscription = subject.subscribe({ next: setValue, complete: () => setValue(null) });
    return () => subscription.unsubscribe();
  }, [subject]);
  return value;
}
