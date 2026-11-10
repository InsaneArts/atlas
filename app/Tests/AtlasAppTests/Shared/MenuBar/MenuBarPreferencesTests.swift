import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("MenuBarPreferences")
struct MenuBarPreferencesTests {
  @Test("the menu bar's icon is kept under its shipped key, and anything missing or unknown reads as the Orbit")
  func theIconIsKept() throws {
    let suite = "atlas.tests.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let preferences = MenuBarPreferences.stored(in: defaults)
    #expect(preferences.loadIcon() == .orbit)
    preferences.saveIcon(.screen)
    #expect(preferences.loadIcon() == .screen)
    #expect(defaults.string(forKey: "menubar.icon") == "screen")
    defaults.set("comet", forKey: "menubar.icon")
    #expect(preferences.loadIcon() == .orbit)
  }
}
