import AppKit
import CoreGraphics

@MainActor
final class SessionStatusIndicatorController {
  private static let defaultScreenMargin: CGFloat = 22

  private let panel: NSPanel
  private let indicatorView: SessionStatusIndicatorView
  private let menuBarStatusItem: NSStatusItem
  private let menuBarClickTarget: MenuBarSessionStatusIndicatorTarget
  private let onActivationRequest: (String) -> Void
  private var hasUserPositionedPanel = false

  /**
   CDXC:SessionStatusIndicators 2026-05-05-19:47
   Session counts must be rendered by AppKit, not SwiftUI, so the floating
   status UI can live outside the ghostex content view, default to the built-in or
   primary display, and support direct drag repositioning without webview hit
   testing.
   */
  init(
    onActivationRequest: @escaping (String) -> Void,
    onClick: @escaping (NativeSessionStatusIndicatorStatus) -> Void
  ) {
    /**
     CDXC:SessionStatusIndicators 2026-05-09-15:48
     The menu bar indicator must be a second presentation of the floating
     status indicator, not a separate state machine. Reuse the same computed
     visible items and click callback so attention, working, and idle session
     counts route through the existing sidebar selector.
     */
    let view = SessionStatusIndicatorView(
      onClick: { status in
        onActivationRequest("floatingStatusIndicatorClick.\(status.rawValue)")
        NSApp.activate(ignoringOtherApps: true)
        onClick(status)
      },
      onDrag: {})
    let menuBarStatusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    let menuBarClickTarget = MenuBarSessionStatusIndicatorTarget(onClick: { status in
      onActivationRequest("menuBarStatusIndicatorClick.\(status.rawValue)")
      NSApp.activate(ignoringOtherApps: true)
      onClick(status)
    })
    let panel = NSPanel(
      contentRect: NSRect(origin: .zero, size: view.preferredSize),
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false
    )
    self.indicatorView = view
    self.menuBarStatusItem = menuBarStatusItem
    self.menuBarClickTarget = menuBarClickTarget
    self.onActivationRequest = onActivationRequest
    self.panel = panel
    panel.backgroundColor = .clear
    panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
    panel.contentView = indicatorView
    panel.hasShadow = false
    panel.hidesOnDeactivate = false
    panel.ignoresMouseEvents = false
    panel.isFloatingPanel = true
    panel.isOpaque = false
    panel.isReleasedWhenClosed = false
    panel.level = .floating
    view.onDrag = { [weak self] in
      self?.hasUserPositionedPanel = true
    }
    menuBarStatusItem.isVisible = false
    if let button = menuBarStatusItem.button {
      button.action = #selector(MenuBarSessionStatusIndicatorTarget.clicked(_:))
      button.imagePosition = .imageOnly
      button.target = menuBarClickTarget
    }
  }

  func apply(_ command: SetSessionStatusIndicators) {
    let items = Self.visibleItems(for: command)
    indicatorView.sizeSetting = command.size
    indicatorView.items = items
    applyMenuBarItems(items, isHidden: command.hideMenuBarIndicators)
    /**
     CDXC:SessionStatusIndicators 2026-05-09-17:30
     Floating badges are hidden by default while menu bar badges remain visible
     by default. Apply both visibility settings after computing shared items so
     hiding a surface never changes counts or click target ordering.
     */
    guard !items.isEmpty && !command.hideFloatingIndicators else {
      panel.orderOut(nil)
      return
    }

    let nextSize = indicatorView.preferredSize
    let nextOrigin =
      hasUserPositionedPanel
      ? Self.clampedOrigin(panel.frame.origin, size: nextSize)
      : Self.defaultOrigin(size: nextSize)
    panel.setFrame(NSRect(origin: nextOrigin, size: nextSize), display: true)
    panel.orderFrontRegardless()
  }

  private static func visibleItems(
    for command: SetSessionStatusIndicators
  ) -> [SessionStatusIndicatorItem] {
    /**
     CDXC:SessionStatusIndicators 2026-05-05-19:47
     Attention and working counts are action states and should suppress the
     idle available-session total whenever either exists. The idle square is
     only a quiet all-available summary for the fully idle case.
     CDXC:SessionStatusIndicators 2026-05-08-09:09
     Floating status badges draw directly on a transparent panel without a
     shared backdrop, so status color must live on each individual badge.
     CDXC:SessionStatusIndicators 2026-05-09-15:53
     Working status badges are `working`, not `running`. Keep native naming
     aligned with app terminology so `running` remains reserved for live
     runtime state and the idle available-session count.

     CDXC:SessionStatusIndicators 2026-06-13-07:20:
     Floating and menu bar status badges must use a single solid square fill
     with no border radius. Use #a0622c for working, #0093fe for attention,
     and #1e1e1e for the idle available-session summary.
     */
    if command.attentionCount > 0 || command.workingCount > 0 {
      return [
        command.attentionCount > 0
          ? SessionStatusIndicatorItem(
            status: .attention,
            count: command.attentionCount,
            color: NSColor(calibratedRed: 0x00 / 255, green: 0x93 / 255, blue: 0xFE / 255, alpha: 1))
          : nil,
        command.workingCount > 0
          ? SessionStatusIndicatorItem(
            status: .working,
            count: command.workingCount,
            color: NSColor(calibratedRed: 0xA0 / 255, green: 0x62 / 255, blue: 0x2C / 255, alpha: 1))
          : nil,
      ].compactMap { $0 }
    }

    guard command.availableCount > 0 else {
      return []
    }
    return [
      SessionStatusIndicatorItem(
        status: .available,
        count: command.availableCount,
        color: NSColor(calibratedRed: 0x1E / 255, green: 0x1E / 255, blue: 0x1E / 255, alpha: 1))
    ]
  }

  private func applyMenuBarItems(_ items: [SessionStatusIndicatorItem], isHidden: Bool) {
    guard !items.isEmpty && !isHidden else {
      menuBarClickTarget.items = []
      menuBarStatusItem.isVisible = false
      return
    }

    let sizeSetting = SessionStatusIndicatorView.menuBarSizeSetting
    let preferredSize = SessionStatusIndicatorView.preferredSize(
      for: items,
      sizeSetting: sizeSetting)
    menuBarClickTarget.items = items
    menuBarClickTarget.sizeSetting = sizeSetting
    menuBarStatusItem.length = preferredSize.width
    menuBarStatusItem.isVisible = true
    guard let button = menuBarStatusItem.button else {
      return
    }
    button.image = SessionStatusIndicatorView.image(for: items, sizeSetting: sizeSetting)
    button.image?.isTemplate = false
    button.toolTip = "Ghostex session status"
  }

  private static func defaultOrigin(size: NSSize) -> NSPoint {
    let screen = defaultScreen()
    let frame = screen.visibleFrame
    return NSPoint(
      x: frame.maxX - size.width - defaultScreenMargin,
      y: frame.minY + defaultScreenMargin)
  }

  private static func clampedOrigin(_ origin: NSPoint, size: NSSize) -> NSPoint {
    guard let screen = screen(containing: origin) ?? defaultScreenOptional() else {
      return origin
    }
    /**
     CDXC:SessionStatusIndicators 2026-05-08-10:22
     User-positioned floating indicators must be allowed in bottom screen
     corners beside the Dock. Clamp manual positions to the full screen frame,
     not visibleFrame, so count/size updates do not push them out of the Dock
     strip after the user places them there.
     */
    let frame = screen.frame
    let maxX = max(frame.minX, frame.maxX - size.width)
    let maxY = max(frame.minY, frame.maxY - size.height)
    return NSPoint(
      x: min(max(origin.x, frame.minX), maxX),
      y: min(max(origin.y, frame.minY), maxY))
  }

  private static func screen(containing origin: NSPoint) -> NSScreen? {
    NSScreen.screens.first { $0.frame.contains(origin) }
  }

  private static func defaultScreen() -> NSScreen {
    defaultScreenOptional() ?? NSScreen.main ?? NSScreen.screens.first!
  }

  private static func defaultScreenOptional() -> NSScreen? {
    NSScreen.screens.first(where: isBuiltInScreen) ?? NSScreen.main ?? NSScreen.screens.first
  }

  private static func isBuiltInScreen(_ screen: NSScreen) -> Bool {
    let key = NSDeviceDescriptionKey("NSScreenNumber")
    guard let displayNumber = screen.deviceDescription[key] as? NSNumber else {
      return false
    }
    return CGDisplayIsBuiltin(CGDirectDisplayID(displayNumber.uint32Value)) != 0
  }
}

private struct SessionStatusIndicatorItem {
  let status: NativeSessionStatusIndicatorStatus
  let count: Int
  let color: NSColor
}

@MainActor
private final class SessionStatusIndicatorView: NSView {
  private struct IndicatorMetrics {
    let scale: CGFloat

    var badgeSide: CGFloat { 52 * scale }
    var horizontalInset: CGFloat { 11 * scale }
    var verticalInset: CGFloat { 8 * scale }
    var itemGap: CGFloat { 6 * scale }
    var minimumTextPadding: CGFloat { 20 * scale }
    var countFont: NSFont {
      NSFont.monospacedDigitSystemFont(ofSize: 25 * scale, weight: .bold)
    }

    var badgeFillInset: CGFloat { 3.5 * scale }
    var textBaselineOffset: CGFloat { 0.5 * scale }
  }

  static let menuBarSizeSetting: NativeSessionStatusIndicatorSize = .small

  private struct DragState {
    let mouseStart: NSPoint
    let windowOriginStart: NSPoint
    var didMove: Bool = false
  }

  /**
   CDXC:SessionStatusIndicators 2026-05-07-16:42
   Counts should read clearly at default size, and a future user-facing size
   setting should scale a small set of base metrics instead of rewriting draw
   logic. Keep the default number visually dominant inside the indicator.
   CDXC:SessionStatusIndicators 2026-05-07-17:36
   Preserve the inactive-only-when-no-action-state visibility rule in
   visibleItems so idle counts appear only when no working or attention badge
   needs priority.
   CDXC:SessionStatusIndicators 2026-05-07-18:02
   A single visible status should remain easy to click and read. Keep explicit
   view padding around each square badge so transparent NSPanel edges do not
   crowd the count.
   CDXC:SessionStatusIndicators 2026-05-07-18:20
   Medium is the default and scales every drawing metric to 50% of X-Large;
   Large and Small are named settings values that reuse the same AppKit drawing
   path.
   CDXC:SessionStatusIndicators 2026-05-07-18:32
   The indicator should fit the visible square badges tightly, including the
   single-badge case, and numbers render as full white rather than tinted text.
   CDXC:SessionStatusIndicators 2026-05-08-09:09
   The floating indicator must not draw a shared background behind the badges.
   Keep the NSPanel and NSView clear so only individual status squares render.
   CDXC:SessionStatusIndicators 2026-05-08-09:17
   Status buttons should not have a gray outer ring. Use a flat colored badge
   so the control does not add extra background colors around the status token.
   CDXC:SessionStatusIndicators 2026-05-08-10:21
   Indicator numbers should render 2px larger at the base drawing scale while
   preserving the existing Small/Medium/Large/X-Large size scaling behavior.
   CDXC:SessionStatusIndicators 2026-05-08-10:27
   Repositioning must not require a Shift modifier. Track ordinary drags from
   mouse-down and reserve click activation for mouse-up without panel movement.
   CDXC:SessionStatusIndicators 2026-06-13-07:20
   Floating and menu bar running-session indicators must render as solid,
   fully square status backgrounds. Do not use rounded paths, gradients,
   strokes, or shadow fills for the badge background because status color must
   stay exactly #a0622c, #0093fe, or #1e1e1e.
   */
  private static func metrics(for size: NativeSessionStatusIndicatorSize) -> IndicatorMetrics {
    switch size {
    case .small:
      return IndicatorMetrics(scale: 0.4)
    case .medium:
      return IndicatorMetrics(scale: 0.5)
    case .large:
      return IndicatorMetrics(scale: 0.75)
    case .xLarge:
      return IndicatorMetrics(scale: 1)
    }
  }

  var items: [SessionStatusIndicatorItem] = [] {
    didSet {
      needsDisplay = true
    }
  }

  var sizeSetting: NativeSessionStatusIndicatorSize = .medium {
    didSet {
      needsDisplay = true
    }
  }

  var preferredSize: NSSize {
    Self.preferredSize(for: items, sizeSetting: sizeSetting)
  }

  static func preferredSize(
    for items: [SessionStatusIndicatorItem],
    sizeSetting: NativeSessionStatusIndicatorSize
  ) -> NSSize {
    let metrics = currentMetrics(for: sizeSetting)
    let itemWidths = items.map { diameter(for: $0, sizeSetting: sizeSetting) }
    let contentWidth =
      itemWidths.reduce(0, +)
      + CGFloat(max(items.count - 1, 0)) * metrics.itemGap
      + metrics.horizontalInset * 2
    let width = contentWidth
    let height = (itemWidths.max() ?? metrics.badgeSide) + metrics.verticalInset * 2
    return NSSize(width: width, height: height)
  }

  private var currentMetrics: IndicatorMetrics {
    Self.metrics(for: sizeSetting)
  }

  private static func currentMetrics(
    for sizeSetting: NativeSessionStatusIndicatorSize
  ) -> IndicatorMetrics {
    metrics(for: sizeSetting)
  }

  private let onClick: (NativeSessionStatusIndicatorStatus) -> Void
  var onDrag: () -> Void
  private var mouseDownStatus: NativeSessionStatusIndicatorStatus?
  private var dragState: DragState?

  init(
    onClick: @escaping (NativeSessionStatusIndicatorStatus) -> Void,
    onDrag: @escaping () -> Void
  ) {
    self.onClick = onClick
    self.onDrag = onDrag
    super.init(frame: NSRect(origin: .zero, size: .zero))
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override var isOpaque: Bool {
    false
  }

  override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
    true
  }

  override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)
    Self.draw(items: items, in: bounds, sizeSetting: sizeSetting)
  }

  static func image(
    for items: [SessionStatusIndicatorItem],
    sizeSetting: NativeSessionStatusIndicatorSize
  ) -> NSImage {
    let size = preferredSize(for: items, sizeSetting: sizeSetting)
    let image = NSImage(size: size)
    image.lockFocus()
    draw(items: items, in: NSRect(origin: .zero, size: size), sizeSetting: sizeSetting)
    image.unlockFocus()
    return image
  }

  private static func draw(
    items: [SessionStatusIndicatorItem],
    in bounds: NSRect,
    sizeSetting: NativeSessionStatusIndicatorSize
  ) {
    let metrics = currentMetrics(for: sizeSetting)
    for (item, rect) in itemRects(items: items, bounds: bounds, sizeSetting: sizeSetting) {
      drawBadge(item, in: rect, metrics: metrics)

      let label = NSAttributedString(
        string: "\(item.count)",
        attributes: textAttributes(metrics: metrics))
      let labelSize = label.size()
      label.draw(
        at: NSPoint(
          x: rect.midX - labelSize.width / 2,
          y: rect.midY - labelSize.height / 2 + metrics.textBaselineOffset))
    }
  }

  private static func drawBadge(
    _ item: SessionStatusIndicatorItem,
    in rect: NSRect,
    metrics: IndicatorMetrics
  ) {
    let badgeRect = rect.insetBy(dx: metrics.badgeFillInset, dy: metrics.badgeFillInset)
    item.color.setFill()
    NSBezierPath(rect: badgeRect).fill()
  }

  private static func textAttributes(metrics: IndicatorMetrics) -> [NSAttributedString.Key: Any] {
    let shadow = NSShadow()
    shadow.shadowBlurRadius = 2 * metrics.scale
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.58)
    shadow.shadowOffset = NSSize(width: 0, height: -1 * metrics.scale)
    return [
      .font: metrics.countFont,
      .foregroundColor: NSColor.white,
      .shadow: shadow,
    ]
  }

  override func mouseDown(with event: NSEvent) {
    mouseDownStatus = nil
    beginDragTracking()
    mouseDownStatus = status(at: convert(event.locationInWindow, from: nil))
  }

  override func mouseDragged(with event: NSEvent) {
    guard let dragState, let window else {
      return
    }
    if !dragState.didMove {
      self.dragState?.didMove = true
      mouseDownStatus = nil
      onDrag()
    }
    let mouseLocation = NSEvent.mouseLocation
    window.setFrameOrigin(
      NSPoint(
        x: dragState.windowOriginStart.x + mouseLocation.x - dragState.mouseStart.x,
        y: dragState.windowOriginStart.y + mouseLocation.y - dragState.mouseStart.y))
  }

  override func mouseUp(with event: NSEvent) {
    if dragState?.didMove == true {
      dragState = nil
      return
    }
    dragState = nil
    guard let mouseDownStatus else {
      return
    }
    defer {
      self.mouseDownStatus = nil
    }
    if status(at: convert(event.locationInWindow, from: nil)) == mouseDownStatus {
      onClick(mouseDownStatus)
    }
  }

  private func beginDragTracking() {
    guard let window else {
      return
    }
    dragState = DragState(
      mouseStart: NSEvent.mouseLocation,
      windowOriginStart: window.frame.origin)
  }

  private func status(at point: NSPoint) -> NativeSessionStatusIndicatorStatus? {
    Self.status(at: point, in: bounds, items: items, sizeSetting: sizeSetting)
  }

  static func status(
    at point: NSPoint,
    in bounds: NSRect,
    items: [SessionStatusIndicatorItem],
    sizeSetting: NativeSessionStatusIndicatorSize
  ) -> NativeSessionStatusIndicatorStatus? {
    itemRects(items: items, bounds: bounds, sizeSetting: sizeSetting)
      .first { _, rect in rect.contains(point) }?.0.status
  }

  private func itemRects() -> [(SessionStatusIndicatorItem, NSRect)] {
    Self.itemRects(items: items, bounds: bounds, sizeSetting: sizeSetting)
  }

  private static func itemRects(
    items: [SessionStatusIndicatorItem],
    bounds: NSRect,
    sizeSetting: NativeSessionStatusIndicatorSize
  ) -> [(SessionStatusIndicatorItem, NSRect)] {
    let metrics = currentMetrics(for: sizeSetting)
    let centerY = bounds.midY
    let itemWidths = items.map { diameter(for: $0, sizeSetting: sizeSetting) }
    let groupWidth =
      itemWidths.reduce(0, +)
      + CGFloat(max(items.count - 1, 0)) * metrics.itemGap
    var x = (bounds.width - groupWidth) / 2
    return items.map { item in
      let diameter = diameter(for: item, sizeSetting: sizeSetting)
      let rect = NSRect(
        x: x,
        y: centerY - diameter / 2,
        width: diameter,
        height: diameter)
      x += diameter + metrics.itemGap
      return (item, rect)
    }
  }

  private func diameter(for item: SessionStatusIndicatorItem) -> CGFloat {
    Self.diameter(for: item, sizeSetting: sizeSetting)
  }

  private static func diameter(
    for item: SessionStatusIndicatorItem,
    sizeSetting: NativeSessionStatusIndicatorSize
  ) -> CGFloat {
    let metrics = currentMetrics(for: sizeSetting)
    let label = NSAttributedString(
      string: "\(item.count)",
      attributes: [.font: metrics.countFont])
    return max(metrics.badgeSide, ceil(label.size().width + metrics.minimumTextPadding))
  }
}

@MainActor
private final class MenuBarSessionStatusIndicatorTarget: NSObject {
  var items: [SessionStatusIndicatorItem] = []
  var sizeSetting: NativeSessionStatusIndicatorSize = SessionStatusIndicatorView.menuBarSizeSetting
  private let onClick: (NativeSessionStatusIndicatorStatus) -> Void

  init(onClick: @escaping (NativeSessionStatusIndicatorStatus) -> Void) {
    self.onClick = onClick
  }

  @objc func clicked(_ sender: NSStatusBarButton) {
    guard !items.isEmpty else {
      return
    }
    let point =
      NSApp.currentEvent.map { sender.convert($0.locationInWindow, from: nil) }
      ?? NSPoint(x: sender.bounds.midX, y: sender.bounds.midY)
    guard
      let status = SessionStatusIndicatorView.status(
        at: point,
        in: sender.bounds,
        items: items,
        sizeSetting: sizeSetting)
    else {
      return
    }
    onClick(status)
  }
}
