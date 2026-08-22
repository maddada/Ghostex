import { describe, expect, test } from "vite-plus/test";
import { createEditorLayoutPlan } from "./editor-layout";

describe("createEditorLayoutPlan", () => {
  test("should build a row-major grid layout for supported counts", () => {
    expect(createEditorLayoutPlan(1, "grid").rowLengths).toEqual([1]);
    expect(createEditorLayoutPlan(2, "grid").rowLengths).toEqual([2]);
    expect(createEditorLayoutPlan(3, "grid").rowLengths).toEqual([2, 1]);
    expect(createEditorLayoutPlan(4, "grid").rowLengths).toEqual([2, 2]);
    expect(createEditorLayoutPlan(5, "grid").rowLengths).toEqual([3, 2]);
    expect(createEditorLayoutPlan(6, "grid").rowLengths).toEqual([3, 3]);
    expect(createEditorLayoutPlan(7, "grid").rowLengths).toEqual([3, 2, 2]);
    expect(createEditorLayoutPlan(8, "grid").rowLengths).toEqual([3, 3, 2]);
    expect(createEditorLayoutPlan(9, "grid").rowLengths).toEqual([3, 3, 3]);
  });

  test("should create a wrapped layout for three-session grid mode", () => {
    const layout = createEditorLayoutPlan(3, "grid");

    expect(layout.layout.orientation).toBe(1);
    expect(layout.layout.groups).toHaveLength(2);
    expect(layout.rowLengths).toEqual([2, 1]);
    expect(layout.layout.groups[0]).toEqual({
      groups: [{}, {}],
      orientation: 0,
    });
    expect(layout.layout.groups[1]).toEqual({
      groups: [{}],
      orientation: 0,
    });
  });

  test("should build grid rows before columns for larger grids", () => {
    const layout = createEditorLayoutPlan(9, "grid");

    expect(layout.layout.orientation).toBe(1);
    expect(layout.rowLengths).toEqual([3, 3, 3]);
    expect(layout.layout.groups).toEqual([
      { groups: [{}, {}, {}], orientation: 0 },
      { groups: [{}, {}, {}], orientation: 0 },
      { groups: [{}, {}, {}], orientation: 0 },
    ]);
  });

  test("should use rows for horizontal mode", () => {
    const layout = createEditorLayoutPlan(6, "horizontal");

    expect(layout.rowLengths).toEqual([1, 1, 1, 1, 1, 1]);
    expect(layout.layout.orientation).toBe(0);
    expect(layout.layout.groups).toHaveLength(6);
  });

  test("should use columns for vertical mode", () => {
    const layout = createEditorLayoutPlan(6, "vertical");

    expect(layout.rowLengths).toEqual([6]);
    expect(layout.layout.orientation).toBe(1);
    expect(layout.layout.groups).toHaveLength(6);
  });
});
