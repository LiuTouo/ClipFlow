import { describe, expect, it } from "vitest";
import { classifyClip, filterItems, isLink, matchesFilter } from "./dataset";
import type { SearchableClip } from "./dataset";

function clip(partial: Partial<SearchableClip>): SearchableClip {
  return {
    kind: "Text",
    text_content: null,
    preview: "",
    source_exe: "app.exe",
    source_title: "",
    ...partial,
  };
}

describe("isLink", () => {
  it("accepts a single http/https URL", () => {
    expect(isLink("https://example.com")).toBe(true);
    expect(isLink("  http://a.b/c ")).toBe(true);
  });
  it("rejects plain text and other schemes", () => {
    expect(isLink("hello")).toBe(false);
    expect(isLink("mailto:a@b.c")).toBe(false);
    expect(isLink(null)).toBe(false);
  });
});

describe("classifyClip", () => {
  it("maps kinds to filters", () => {
    expect(classifyClip(clip({ kind: "Image" }))).toBe("image");
    expect(classifyClip(clip({ kind: "FilePaths" }))).toBe("files");
    expect(classifyClip(clip({ text_content: "https://x.dev" }))).toBe("links");
    expect(classifyClip(clip({ text_content: "plain" }))).toBe("text");
  });
});

describe("matchesFilter", () => {
  it("all passes everything", () => {
    expect(matchesFilter(clip({ kind: "Image" }), "all")).toBe(true);
  });
  it("category filter matches only its kind", () => {
    expect(matchesFilter(clip({ kind: "Image" }), "image")).toBe(true);
    expect(matchesFilter(clip({ kind: "Image" }), "text")).toBe(false);
  });
});

describe("filterItems", () => {
  const items = [
    clip({ preview: "alpha", source_exe: "Code.exe", kind: "Text" }),
    clip({ preview: "beta", source_exe: "Notes.exe", kind: "Text" }),
    clip({ preview: "Image", kind: "Image" }),
  ];

  it("applies search across preview/source/title", () => {
    expect(filterItems(items, "alpha", "all").map((c) => c.preview)).toEqual(["alpha"]);
    expect(filterItems(items, "code", "all").map((c) => c.source_exe)).toEqual(["Code.exe"]);
  });

  it("combines filter and search", () => {
    // "alpha" is Text and matches; "Image" is filtered out by the "text" filter.
    expect(filterItems(items, "alpha", "text").map((c) => c.preview)).toEqual(["alpha"]);
    // The "image" filter ignores the two Text items.
    expect(filterItems(items, "", "image").map((c) => c.preview)).toEqual(["Image"]);
  });

  it("returns everything for an empty query and all filter", () => {
    expect(filterItems(items, "", "all")).toHaveLength(3);
  });
});
