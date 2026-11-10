# Coordinator captain's hat: where the artwork comes from

2026-11-10. The user supplied the picture and holds the licence for it: "im licensed to use that image pls use it directly".

- `coordinator-captain-hat.svg` is that picture (a 512 px black line drawing of a peaked captain's cap) vectorized with potrace 1.16 (`--turdsize 2 --alphamax 1.0 --opttolerance 0.2`), scaled to 23 units wide and centred in the 24 viewBox, filled with `currentColor`. The drawing is not changed.
- `coordinator-captain-hat-small.svg` is the same picture with its lines thickened by 3 px (at 512 px, a disk erosion of the thresholded bitmap) before the same trace and fit, for sizes of 17 px and below, where the original's half-pixel lines go faint on a 1x screen. Nothing is redrawn.
- The phone draws the first one (`CoordinatorGlyph` in apps/mobile/app/src/components/sessions/icons.tsx).

It replaced a hand-drawn cap the user rejected ("the icon u put for orchestrators doesn't look like the one i shared, this one looks simpler and uglier please fix").
