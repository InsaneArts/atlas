import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("Panel and viewer icon delivery")
struct PanelControllerIconsTests {
  private static let bundleId = "test.icons.shared"
  private static let bundle = "/Applications/Shared.app"

  @Test("the panel and viewer share one lookup and a reopened viewer reuses its decoded image")
  func sharesAcrossWindows() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    let entry = ActivityLog.Entry(id: "shared", says: "Shared", app: "Shared", bundleId: Self.bundleId)
    viewer.panesModel.wantIcons(for: [entry])
    panel.wantRecentIcons()
    try await settle("the windows never shared their request") { icons.repository.waiterCount == 2 && gate.count == 1 }
    #expect(gate.count == 1)
    gate.release()
    let key = try #require(AppIconKey(bundleId: Self.bundleId))
    try await settle("the shared image never reached both windows") {
      panel.icons[key.text] != nil && viewer.panes.icons.images[key.text] != nil
    }
    let image = try #require(panel.icons[key.text])
    #expect(viewer.panes.icons.images[key.text]?.image === image.image)
    viewer.close()
    let reopened = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { reopened.close() }
    reopened.panesModel.wantIcons(for: [entry])
    try await settle("the reopened viewer never received the cached icon") {
      reopened.panes.icons.images[key.text] != nil
    }
    #expect(reopened.panes.icons.images[key.text]?.image === image.image)
    #expect(gate.count == 1)
  }

  @Test("a lookup still running when its viewer closes cannot write stale results or retain the window")
  func dropsClosedViewerResults() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    var viewer: ViewerController? = try await ViewerController.watching(ScriptedService(), icons: icons)
    let releasedViewer = WeakViewer()
    releasedViewer.controller = viewer
    let panes = try #require(viewer?.panesModel)
    panes.wantIcons(for: [ActivityLog.Entry(id: "1", says: "Shared", app: "Shared", bundleId: Self.bundleId)])
    try await settle("the icon read never started") { gate.count == 1 }
    viewer?.close()
    viewer = nil
    #expect(releasedViewer.controller == nil, "the outstanding lookup captures the viewer weakly")
    try await settle("the closed viewer kept its waiter") { icons.repository.waiterCount == 0 }
    gate.release()
    try await settle("the lookup never finished") { icons.repository.cachedCount == 1 }
    // Await another delivery after the original receiver, so this assertion observes the late callback as well.
    let want = try #require(AppIconWant.named(bundleId: Self.bundleId, name: "Shared"))
    await icons.request(want) { _, _ in }.value
    #expect(panes.panes.icons.images.isEmpty)
    #expect(icons.repository.waiterCount == 0)
  }

  @Test("an unwanted icon or a closed panel drops late icon delivery", arguments: [true, false])
  func dropsObsoletePanelResults(close: Bool) async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    panel.isOpen = true
    panel.wantRecentIcons()
    try await settle("the panel lookup never started") { gate.count == 1 }
    let previous = panel.status
    if close {
      panel.watch(open: false)
    } else {
      panel.status = ViewerStatus()
      panel.wantRecentIcons()
    }
    try await settle("the panel kept its unwanted waiter") { icons.repository.waiterCount == 0 }
    gate.release()
    try await settle("the lookup never finished") { icons.repository.cachedCount == 1 }
    let want = try #require(AppIconWant.named(bundleId: Self.bundleId, name: "Shared"))
    await icons.request(want) { _, _ in }.value
    #expect(panel.icons.isEmpty)
    panel.isOpen = true
    panel.status = previous
    panel.wantRecentIcons()
    try await settle("the next panel reading never reused the cached icon") { !panel.icons.isEmpty }
    #expect(gate.count == 1)
  }

  @Test("closing a window cancels its own icon waits and leaves another window's", arguments: [true, false])
  func closesOnlyItsOwnRequests(closeViewer: Bool) async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    let entry = ActivityLog.Entry(id: "shared", says: "Shared", app: "Shared", bundleId: Self.bundleId)
    viewer.panesModel.wantIcons(for: [entry])
    panel.wantRecentIcons()
    try await settle("the windows never joined the lookup") { icons.repository.waiterCount == 2 && gate.count == 1 }
    if closeViewer { viewer.close() } else { panel.watch(open: false) }
    try await settle("closing a window did not remove just its waiter") { icons.repository.waiterCount == 1 }
    #expect(gate.active == 1)
    gate.release()
    try await settle("the remaining window never received its icon") {
      closeViewer ? !panel.icons.isEmpty : !viewer.panes.icons.images.isEmpty
    }
    #expect(closeViewer ? viewer.panes.icons.images.isEmpty : panel.icons.isEmpty)
    #expect(icons.repository.waiterCount == 0)
    #expect(gate.count == 1)
  }

  @Test("polling during a slow lookup adds no more waiters for the same icon")
  func boundsWaitersAcrossPolls() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    let entry = ActivityLog.Entry(id: "shared", says: "Shared", app: "Shared", bundleId: Self.bundleId)
    viewer.panesModel.wantIcons(for: [entry])
    panel.wantRecentIcons()
    try await settle("the windows never joined the lookup") { icons.repository.waiterCount == 2 && gate.count == 1 }
    for _ in 0..<20 {
      viewer.panesModel.wantIcons(for: [entry, entry])
      panel.statusReading += 1
      panel.wantRecentIcons()
      await Task.yield()
      #expect(icons.repository.waiterCount == 2)
      #expect(viewer.panesModel.iconRequests.count == 1)
      #expect(panel.iconRequests.count == 1)
    }
    gate.release()
    try await settle("a retained request did not deliver after newer polls") {
      !panel.icons.isEmpty && !viewer.panes.icons.images.isEmpty
    }
    #expect(panel.iconRequests.isEmpty)
    #expect(viewer.panesModel.iconRequests.isEmpty)
    #expect(gate.count == 1)
  }

  @Test("the viewer changing its place cancels its icon waits while the panel keeps waiting")
  func cancelsWhenLeavingAPane() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    viewer.panesModel.wantIcons(for: [
      ActivityLog.Entry(id: "1", says: "Shared", app: "Shared", bundleId: Self.bundleId)
    ])
    panel.wantRecentIcons()
    try await settle("the windows never joined the lookup") { icons.repository.waiterCount == 2 }
    viewer.show(.activity)
    try await settle("the old place kept its icon waiter") { icons.repository.waiterCount == 1 }
    gate.release()
    try await settle("the panel never received the icon") { !panel.icons.isEmpty }
    #expect(viewer.panes.icons.images.isEmpty)
  }

  @Test("an app disappearing from a viewer reading cancels only that viewer's icon wait")
  func cancelsUnwantedViewerIcons() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    let icons = icons(gate)
    let panel = try panel(icons)
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    let entry = ActivityLog.Entry(id: "shared", says: "Shared", app: "Shared", bundleId: Self.bundleId)
    viewer.panesModel.wantIcons(for: [entry])
    panel.wantRecentIcons()
    try await settle("the windows never joined the lookup") { icons.repository.waiterCount == 2 }
    viewer.panesModel.wantIcons(for: [ActivityLog.Entry]())
    try await settle("the viewer kept an unwanted waiter") { icons.repository.waiterCount == 1 }
    gate.release()
    try await settle("the panel never received the icon") { !panel.icons.isEmpty }
    #expect(viewer.panes.icons.images.isEmpty)
  }

  private func icons(_ gate: IconLookupGate) -> AppIcons {
    let image = png()
    let bundle = Self.bundle
    var lookup = AppIconLookup.none
    lookup.bundleOfId = { _ in
      await gate.read()
      return bundle
    }
    lookup.icon = { _ in image }
    return AppIcons(store: AppIconStore(caches: nil), lookup: lookup)
  }

  private func panel(_ icons: AppIcons) throws -> PanelController {
    let panel = PanelController.scripted(icons: icons)
    panel.isOpen = true
    panel.status = try status([
      "overview": [
        "mode": "agent", "state": "working",
        "activity": [["id": "shared", "says": "Shared", "app": "Shared", "bundleId": Self.bundleId]],
      ]
    ])
    return panel
  }
}
