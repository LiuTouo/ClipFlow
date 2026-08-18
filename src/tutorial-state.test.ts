import { describe, expect, it } from "vitest";
import { TutorialNav, TUTORIAL_PAGES, TutorialSession } from "./tutorial-state";

describe("TutorialNav", () => {
  it("starts on the first page", () => {
    const nav = new TutorialNav();
    expect(nav.isFirst).toBe(true);
    expect(nav.current).toBe(0);
  });

  it("advances through pages and stops at the last", () => {
    const nav = new TutorialNav(3);
    expect(nav.next()).toBe(true);
    expect(nav.next()).toBe(true);
    expect(nav.isLast).toBe(true);
    expect(nav.next()).toBe(false);
    expect(nav.current).toBe(2);
  });

  it("goes back and stops at the first", () => {
    const nav = new TutorialNav(3);
    nav.goto(2);
    expect(nav.back()).toBe(true);
    expect(nav.back()).toBe(true);
    expect(nav.isFirst).toBe(true);
    expect(nav.back()).toBe(false);
  });

  it("clamps goto to the valid range", () => {
    const nav = new TutorialNav(3);
    nav.goto(99);
    expect(nav.current).toBe(2);
    nav.goto(-5);
    expect(nav.current).toBe(0);
  });
});

describe("pages", () => {
  it("declares seven pages in order", () => {
    expect(TUTORIAL_PAGES).toHaveLength(7);
    expect(TUTORIAL_PAGES[0].id).toBe("background");
    expect(TUTORIAL_PAGES[6].id).toBe("settings");
  });
});

describe("TutorialSession completion guard", () => {
  it("complete once -> reopen -> Skip works again", () => {
    const s = new TutorialSession();
    expect(s.beginCompletion()).toBe(true);
    s.settle(true); // success; the window hides
    expect(s.isFinished).toBe(true);
    s.reopen(); // tray reopens the reused window
    expect(s.isFinished).toBe(false);
    expect(s.beginCompletion()).toBe(true); // Skip actionable again
  });

  it("complete once -> reopen -> Start works (nav position irrelevant)", () => {
    const s = new TutorialSession();
    expect(s.beginCompletion()).toBe(true);
    s.settle(true);
    s.reopen();
    expect(s.beginCompletion()).toBe(true);
  });

  it("a failed completion re-arms for retry", () => {
    const s = new TutorialSession();
    expect(s.beginCompletion()).toBe(true);
    s.settle(false); // invoke rejected
    expect(s.isFinished).toBe(false);
    expect(s.isSubmitting).toBe(false);
    expect(s.beginCompletion()).toBe(true); // retry allowed
  });

  it("double click while pending submits only once", () => {
    const s = new TutorialSession();
    expect(s.beginCompletion()).toBe(true);
    expect(s.beginCompletion()).toBe(false);
    expect(s.beginCompletion()).toBe(false);
    s.settle(true);
  });

  it("reopen is a no-op while an invoke is in flight", () => {
    const s = new TutorialSession();
    s.beginCompletion();
    s.reopen(); // must not clear the in-flight guard
    expect(s.isFinished).toBe(true);
    expect(s.isSubmitting).toBe(true);
    expect(s.beginCompletion()).toBe(false);
  });
});
