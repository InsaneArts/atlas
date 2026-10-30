import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPanesModel icons")
struct ViewerPanesIconsTests {
  /// Notes, where it is installed.
  static let notes = "/System/Applications/Notes.app"

  @Test("Activity retains every active icon beyond the shared cache limit without reading unchanged apps again")
  func activeIconsOutliveCacheEviction() async throws {
    let count = AppIconRepository.memoryLimit + 1
    let data = png(width: 16, height: 16)
    let entries = (0..<count)
      .map {
        ActivityLog.Entry(id: "\($0)", says: "clicked \($0)", app: "App \($0)", bundleId: "test.app.\($0)")
      }
    let pairs = (0..<count)
      .map { ("test.app.\($0)", "/Applications/App\($0).app") }
    let bundles = Dictionary(uniqueKeysWithValues: pairs)
    let lookups = IconLookups()
    let icons = AppIcons(
      store: AppIconStore(caches: nil),
      lookup: lookups.lookup(icons: bundles.values.reduce(into: [:]) { $0[$1] = data }, ids: bundles),
      home: "/Users/person"
    )
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    defer { viewer.close() }
    viewer.panesModel.wantIcons(for: entries)
    try await settle("the active icons never finished loading") {
      viewer.panes.icons.images.count == count && viewer.panesModel.iconRequests.isEmpty
    }
    #expect(icons.repository.cachedCount == AppIconRepository.memoryLimit)
    let reads = await lookups.asked
    #expect(reads.count == count * 2)
    for _ in 0..<3 {
      viewer.panesModel.wantIcons(for: entries)
      #expect(viewer.panesModel.iconRequests.isEmpty)
      #expect(entries.allSatisfy { viewer.panes.icons.image(bundleId: $0.bundleId, name: $0.app ?? "") != nil })
    }
    #expect(await lookups.asked == reads)

    let remaining = Array(entries.dropFirst())
    viewer.panesModel.wantIcons(for: remaining)
    #expect(viewer.panes.icons.images.count == remaining.count)
    #expect(viewer.panes.icons.images["bundle:test.app.0"] == nil)
    #expect(viewer.panesModel.iconRequests.isEmpty)
    viewer.panesModel.wantIcons(for: [ActivityLog.Entry]())
    #expect(viewer.panes.icons.images.isEmpty, "images leave the presentation when no longer active")
  }

  @Test("an app draws its initial until its icon is found, then its icon, and later readings look for it no more")
  func foundOnce() async throws {
    let service = ScriptedService()
    service.openApps = [HelperApp(pid: 3, name: "Notes", bundle: Self.notes, bundleId: "com.apple.Notes")]
    let lookups = IconLookups()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(
      service,
      clock: clock,
      icons: AppIcons(
        store: AppIconStore(caches: nil),
        lookup: lookups.lookup(icons: [Self.notes: png(width: 128, height: 128)]),
        home: "/Users/person"
      )
    )
    viewer.send(.show(.apps))
    try await settle("the icon was never found") { !viewer.panes.icons.images.isEmpty }
    guard case .apps(let apps) = viewer.state.pane, case .image = apps.cards.first?.icon else {
      Issue.record("Notes does not draw its icon")
      return
    }
    for reading in 2...3 {
      try await clock.untilSleeping(for: ViewerPanesModel.appsEvery, "Apps never paused before reading again")
      clock.advance(by: ViewerPanesModel.appsEvery)
      try await settle("the apps were not read again") { service.appsCalls == reading }
    }
    #expect(await lookups.asked == ["icon \(Self.notes)"])
    viewer.close()
  }

  @Test("an app whose icon is not found keeps its initial and is not looked for again")
  func missedOnce() async throws {
    let service = ScriptedService()
    service.openApps = [HelperApp(pid: 9, name: "Probe", bundleId: "test.atlas.probe")]
    let lookups = IconLookups()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(
      service,
      clock: clock,
      icons: AppIcons(store: AppIconStore(caches: nil), lookup: lookups.lookup(), home: "/Users/person")
    )
    viewer.send(.show(.apps))
    try await settle("the apps were never read") { service.appsCalls == 1 }
    try await Task { await lookups.until(1) }
      .value("the icon was never looked for", within: Ceiling.work)
    try await clock.untilSleeping(for: ViewerPanesModel.appsEvery, "Apps never paused before reading again")
    clock.advance(by: ViewerPanesModel.appsEvery)
    try await settle("the apps were not read again") { service.appsCalls == 2 }
    #expect(await lookups.asked == ["id test.atlas.probe"])
    guard case .apps(let apps) = viewer.state.pane else {
      Issue.record("Apps does not show")
      return
    }
    #expect(apps.cards.first?.icon == .monogram(AppMonogram(name: "Probe")))
    viewer.close()
  }

  @Test("new live evidence retries an earlier Activity icon miss")
  func liveEvidenceRetriesAMiss() async throws {
    let icons = AppIcons.none
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    let weak = try #require(AppIconWant.named(bundleId: "test.atlas.probe", name: "Probe"))
    #expect(await icons.find(weak) == nil)
    let app = HelperApp(pid: 9, name: "Probe", bundleId: "test.atlas.probe", icon: png(width: 128, height: 128))
    viewer.panesModel.wantIcons(for: [app])
    try await settle("the live icon never replaced the miss") { viewer.panes.icons.images[weak.key.text] != nil }
    viewer.close()
  }

  @Test("live icon work waits behind a weaker in-flight lookup and finishes only after processing")
  func liveEvidenceWaitsBehindAWeakerLookup() async throws {
    let gate = IconLookupGate()
    defer { gate.release() }
    var lookup = AppIconLookup.none
    lookup.bundleOfId = { _ in
      await gate.read()
      return nil
    }
    let icons = AppIcons(store: AppIconStore(caches: nil), lookup: lookup, home: "/Users/person")
    let viewer = try await ViewerController.watching(ScriptedService(), icons: icons)
    let weak = try #require(AppIconWant.named(bundleId: "test.atlas.probe", name: "Probe"))
    viewer.panesModel.wantIcons(for: [ActivityLog.Entry(id: "1", says: "Probe", app: "Probe", bundleId: weak.bundleId)])
    try await settle("the weaker lookup never started") { gate.count == 1 }
    let app = HelperApp(pid: 9, name: "Probe", bundleId: "test.atlas.probe", icon: png(width: 128, height: 128))
    let bundled = HelperApp(pid: 9, name: "Probe", bundle: "/Applications/Probe.app", bundleId: "test.atlas.probe")
    viewer.panesModel.wantIcons(for: [bundled])
    viewer.panesModel.wantIcons(for: [app])
    viewer.panesModel.wantIcons(for: [app])
    #expect(icons.repository.waiterCount == 1, "new evidence updates this window's existing waiter")
    #expect(viewer.panesModel.iconRequests.count == 1)
    #expect(viewer.panes.icons.images[weak.key.text] == nil)
    gate.release()
    try await settle("the stronger icon request was lost") { viewer.panes.icons.images[weak.key.text] != nil }
    #expect(gate.count == 1)
    viewer.close()
  }

  @Test("an Activity row's app is looked for once by its bundle id")
  func activityRows() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(
        id: "1",
        says: "clicked Save in Notes",
        operation: "input",
        app: "Notes",
        bundleId: "com.apple.Notes"
      ),
      ActivityLog.Entry(id: "2", says: "typed in Notes", operation: "input", app: "Notes", bundleId: "com.apple.Notes"),
    ])
    let lookups = IconLookups()
    let viewer = try await ViewerController.watching(
      service,
      icons: AppIcons(
        store: AppIconStore(caches: nil),
        lookup: lookups.lookup(icons: [Self.notes: png(width: 128, height: 128)], ids: ["com.apple.Notes": Self.notes]),
        home: "/Users/person"
      )
    )
    viewer.send(.show(.activity))
    try await settle("the icon was never found") { !viewer.panes.icons.images.isEmpty }
    #expect(await lookups.asked == ["id com.apple.Notes", "icon \(Self.notes)"])
    viewer.close()
  }
}
