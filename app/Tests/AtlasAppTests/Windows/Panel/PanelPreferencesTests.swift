import Foundation
import Testing

@testable import AtlasApp

@Suite("PanelPreferences")
struct PanelPreferencesTests {
  @Test("the service note's OK is kept in the defaults under its shipped key, and nothing is kept before it")
  func okIsKept() throws {
    let suite = "atlas.tests.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let preferences = PanelPreferences.stored(in: defaults)
    #expect(!preferences.loadServiceNoteDismissed())
    preferences.saveServiceNoteDismissed()
    #expect(preferences.loadServiceNoteDismissed())
    #expect(defaults.bool(forKey: "panel.serviceNoteDismissed"))
  }
}
