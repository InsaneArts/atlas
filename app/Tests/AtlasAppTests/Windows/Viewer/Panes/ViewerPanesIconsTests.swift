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
