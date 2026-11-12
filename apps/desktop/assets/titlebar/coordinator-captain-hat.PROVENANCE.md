# Coordinator captain's hat: where the artwork comes from

2026-11-10. The coordinator's mark is our own drawing after a picture the user supplied and holds the licence for ("im licensed to use that image pls use it directly"): a black line drawing of a peaked captain's cap seen from the side.

- `coordinator-captain-hat.svg` is that same cap drawn by hand seen straight from the front, so the icon is symmetrical (user: "keep the same hat exactly just make it shown from front I mean it will look better for the icon to be symmetrical"; he picked candidate A, "Balanced", in Q2172). It keeps his picture's line weight (0.95 at 24) and rounded pen ends: a saucer top a little wider than the band, the badge with its small inner mark, the band with its stripe, and a curved peak. Stroked in `currentColor`.
- `coordinator-captain-hat-small.svg` is the same drawing with slightly thicker lines (1.25), for 17 px and below, where 0.95 goes faint on a 1x screen.
- The phone draws the first one (`CoordinatorGlyph` in apps/mobile/app/src/components/sessions/icons.tsx).

History: the side view traced from his picture (potrace) was the mark from 177cfd9f2 until this front view replaced it; it stays in git history only. Before that, a hand-drawn side view the user rejected ("this one looks simpler and uglier please fix").

2026-11-12: coordinators now wear the outline crown (`coordinator-crown-points.svg`, the user's pick A in Q2650; CDXC:Coordinators 2026-11-12 in apps/desktop/src/app/native_sidebar/threads.rs). Nothing draws the hat any more; its two drawings stay here for history.
