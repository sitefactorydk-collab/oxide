import { describe, expect, it } from "vitest";
import { MESSAGE_PAGE, windowStart } from "./messageWindow";

describe("message window", () => {
  it("mounts the newest messages of a long chat", () => {
    expect(windowStart(10, MESSAGE_PAGE)).toBe(0);
    expect(windowStart(500, MESSAGE_PAGE)).toBe(500 - MESSAGE_PAGE);
    expect(windowStart(500, 2 * MESSAGE_PAGE)).toBe(500 - 2 * MESSAGE_PAGE);
    expect(windowStart(500, 10_000)).toBe(0);
    expect(windowStart(0, MESSAGE_PAGE)).toBe(0);
  });
});
