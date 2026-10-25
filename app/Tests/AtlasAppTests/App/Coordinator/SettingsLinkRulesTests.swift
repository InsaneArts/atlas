import AtlasServiceTesting
import AtlasUI
import Observation
import Testing
import os

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

  @Test("the picture choice in Settings does not observe incoming pictures")
  func thePictureChoiceDoesNotObservePictures() throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let picture = try #require(SampleDesktop.picture)
    let redrawn = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      #expect(SettingsLinkRules.picture(viewer: viewer, savedRate: 5) == .live)
    } onChange: {
      redrawn.withLock { $0 = true }
    }
    viewer.readingLoop.begin(with: nil, picture: picture)
    #expect(viewer.reading.picture == picture)
    #expect(!redrawn.withLock { $0 })
  }

  @Test("the picture choice in Settings observes pause and cadence changes")
  func thePictureChoiceObservesPauseAndCadence() {
    let viewer = ViewerController.scripted(ScriptedService())
    for choice in [ViewerPictureChoice.everyFiveSeconds, .paused, .live] {
      let redrawn = OSAllocatedUnfairLock(initialState: false)
      withObservationTracking {
        _ = SettingsLinkRules.picture(viewer: viewer, savedRate: 0)
      } onChange: {
        redrawn.withLock { $0 = true }
      }
      SettingsLinkRules.choose(choice, viewer: viewer, preferences: .memory())
      #expect(redrawn.withLock { $0 })
      #expect(SettingsLinkRules.picture(viewer: viewer, savedRate: 0) == choice)
    }
  }
}
