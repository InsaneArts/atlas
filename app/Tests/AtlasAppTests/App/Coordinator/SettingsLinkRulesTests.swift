import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("SettingsLinkRules")
struct SettingsLinkRulesTests {
  @Test("a picture choice made in Settings with no viewer open is saved for the next viewer")
  func aChoiceWithNoViewerIsSaved() {
    let preferences = ViewerPreferences.memory()
    SettingsLinkRules.choose(.everyFiveSeconds, viewer: nil, preferences: preferences)
    #expect(preferences.loadRate() == ViewerPictureRate.everyFiveSeconds.seconds)
    #expect(SettingsLinkRules.picture(viewer: nil, savedRate: preferences.loadRate()) == .everyFiveSeconds)
    SettingsLinkRules.choose(.paused, viewer: nil, preferences: preferences)
    #expect(preferences.loadRate() == ViewerPictureRate.everyFiveSeconds.seconds)
  }

  @Test("a picture choice made in Settings with the viewer open changes its rate")
  func aChoiceWithTheViewerOpenChangesItsRate() {
    let preferences = ViewerPreferences.memory()
    let viewer = ViewerController.scripted(ScriptedService())
    SettingsLinkRules.choose(.everyFiveSeconds, viewer: viewer, preferences: preferences)
    #expect(viewer.snapshot.rate == .everyFiveSeconds)
    #expect(SettingsLinkRules.picture(viewer: viewer, savedRate: 0) == .everyFiveSeconds)
    #expect(preferences.loadRate() == 0)
  }
}
