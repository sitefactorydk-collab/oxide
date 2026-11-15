import { useCallback, useLayoutEffect, useRef, useState, type RefObject } from "react";

/**
 * Long chats mount only their newest messages (Phase 18): rendering Markdown, math and
 * highlighted code for hundreds of messages is what makes a long chat slow to open. Older
 * messages mount a page at a time as the user scrolls up (or clicks "Show earlier
 * messages"), keeping the scroll position. Each mounted message also uses
 * `content-visibility: auto`, so the browser skips laying out and painting the ones off
 * screen.
 */
export const MESSAGE_PAGE = 40;
/** Distance from the top (px) that mounts the next page. */
const LOAD_DISTANCE = 800;

/** Index of the first mounted message. */
export function windowStart(total: number, shown: number): number {
  return Math.max(0, total - Math.max(shown, 0));
}

export function useMessageWindow(total: number, scrollRef: RefObject<HTMLDivElement | null>) {
  const [shown, setShown] = useState(MESSAGE_PAGE);
  const start = windowStart(total, shown);
  const anchor = useRef<number | null>(null);

  const showEarlier = useCallback(() => {
    const el = scrollRef.current;
    anchor.current = el ? el.scrollHeight - el.scrollTop : null;
    setShown((s) => s + MESSAGE_PAGE);
  }, [scrollRef]);

  // Keep what the user was reading in place when earlier messages appear above it.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && anchor.current !== null) {
      el.scrollTop = el.scrollHeight - anchor.current;
      anchor.current = null;
    }
  }, [shown, scrollRef]);

  /** Call from the scroll handler. */
  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (el && start > 0 && anchor.current === null && el.scrollTop < LOAD_DISTANCE) {
      showEarlier();
    }
  }, [scrollRef, start, showEarlier]);

  return { start, hidden: start, showEarlier, onScroll };
}
