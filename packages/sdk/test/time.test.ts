import { describe, expect, it } from "vitest";
import { frames, ms, seconds } from "../src/time";

describe("time sugar", () => {
  it("seconds() rounds to the nearest tick", () => {
    expect(seconds(0.9)).toBe(635_040_000);
  });

  it("ms() converts exactly", () => {
    expect(ms(150)).toBe(105_840_000);
  });

  it("frames(n).at(fps) converts exactly when fps divides the timebase", () => {
    expect(frames(27).at(30)).toBe(27 * 23_520_000);
  });

  it("seconds() throws when the result is not a safe integer", () => {
    expect(() => seconds(1e300)).toThrow();
  });

  it("frames(n).at(fps) throws when fps does not divide the timebase", () => {
    expect(() => frames(1).at(11)).toThrow();
  });

  it("seconds() throws on negative input", () => {
    expect(() => seconds(-1)).toThrow();
  });
});
