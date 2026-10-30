import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite(
  "Viewer screenshot key dispatch",
  .serialized,
  AppKitState(),
  .enabled(
    if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
    "requires the hidden helper graphical test host"
  )
)
@MainActor
struct ViewerScreenshotKeyDispatchTests {
  @Test("Escape returns focus to the screenshot grid so Space can open the picture again")
  func closingRestoresGridFocus() async throws {
    let viewer = Self.viewer()
    let window = ViewerWindow(controller: viewer) { _ in }
    defer { window.close() }
    window.makeKeyAndOrderFront(nil)
    try await Self.render(window) { window.screenshotFocused }
    window.sendEvent(try Self.key(ShotKeys.escape, characters: "\u{1b}", in: window))
    try #require(viewer.panes.shots.large == nil)
    try await Self.render(window) { window.screenshotFocused }
    window.sendEvent(try Self.key(ShotKeys.space, characters: " ", in: window))
    #expect(viewer.panes.shots.large == "shot.png")
  }

  @Test(
    "Copy and Show in Finder own Space after focus moves from the large picture to their buttons",
    arguments: ["Copy", "Show in Finder"]
  )
  func focusedButtonsKeepSpace(_ label: String) async throws {
    var activated = 0
    let viewer = Self.viewer(outlet: ShotOutlet(copy: { _ in activated += 1 }, reveal: { _ in activated += 1 }))
    let window = ViewerWindow(controller: viewer) { _ in }
    defer { window.close() }
    window.makeKeyAndOrderFront(nil)
    try await Self.render(window) { window.screenshotFocused }
    for _ in 0..<20 {
      if Self.focusedLabel() == label { break }
      window.selectNextKeyView(nil)
      await afterQueuedWork()
      window.contentView?.layoutSubtreeIfNeeded()
    }
    try #require(Self.focusedLabel() == label, "the real screenshot button must own native keyboard focus")
    window.sendEvent(try Self.key(ShotKeys.space, characters: " ", in: window))
    try await Self.render(window) { activated == 1 }
    #expect(viewer.panes.shots.large == "shot.png", "a button's activation key cannot toggle the picture")
  }

  private static func viewer(outlet: ShotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })) -> ViewerController {
    let viewer = ViewerController.scripted(ScriptedService(), shotOutlet: outlet)
    viewer.session.place = .screenshots
    viewer.panesModel.panes.shots.shots = [Screenshot(name: "shot.png", takenAt: Date(), width: 1440, height: 900)]
    viewer.panesModel.panes.shots.folder = URL(fileURLWithPath: "/private/tmp")
    viewer.panesModel.panes.shots.large = "shot.png"
    viewer.panesModel.panes.shots.full = ViewerFullShot(png: Data([1]), picture: nil)
    return viewer
  }

  private static func focusedLabel() -> String? {
    let element = NSApplication.accessibilityFocusedUIElement() as? any NSAccessibilityProtocol
    return element?.accessibilityLabel()
  }

  private static func render(_ window: NSWindow, until done: () -> Bool) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    while !done(), ContinuousClock.now < deadline {
      window.contentView?.layoutSubtreeIfNeeded()
      window.displayIfNeeded()
      await afterQueuedWork()
    }
    try #require(done(), "the screenshot surface never took focus")
  }

  private static func key(_ code: UInt16, characters: String, in window: NSWindow) throws -> NSEvent {
    try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: [],
        timestamp: .zero,
        windowNumber: window.windowNumber,
        context: nil,
        characters: characters,
        charactersIgnoringModifiers: characters,
        isARepeat: false,
        keyCode: code
      )
    )
  }
}
