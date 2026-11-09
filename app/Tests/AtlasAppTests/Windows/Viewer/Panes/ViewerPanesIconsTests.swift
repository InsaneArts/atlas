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
      lookup: lookups.lookup(icons: bundles.values.reduce(into: [:]) { $0[$1] = data }, ids: bundles)
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

  @Test("an Activity row's app whose icon is not found is not looked for again")
  func missedOnce() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(
        id: "1",
        says: "clicked in Probe",
        operation: "input",
        app: "Probe",
        bundleId: "test.atlas.probe"
      )
    ])
    let lookups = IconLookups()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(
      service,
      clock: clock,
      icons: AppIcons(store: AppIconStore(caches: nil), lookup: lookups.lookup())
    )
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { service.activityCalls == 1 }
    try await Task { await lookups.until(1) }
      .value("the icon was never looked for", within: Ceiling.work)
    try await clock.untilSleeping(for: ViewerPanesModel.activityEvery, "Activity never paused before reading again")
    clock.advance(by: ViewerPanesModel.activityEvery)
    try await settle("Activity was not read again") { service.activityCalls == 2 }
    #expect(await lookups.asked == ["id test.atlas.probe"])
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
        lookup: lookups.lookup(icons: [Self.notes: png(width: 128, height: 128)], ids: ["com.apple.Notes": Self.notes])
      )
    )
    viewer.send(.show(.activity))
    try await settle("the icon was never found") { !viewer.panes.icons.images.isEmpty }
    #expect(await lookups.asked == ["id com.apple.Notes", "icon \(Self.notes)"])
    viewer.close()
  }
}
