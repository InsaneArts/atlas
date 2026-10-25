import Testing
import UserNotifications

@testable import AtlasApp

@Suite("NotificationClicks")
struct NotificationClicksTests {
  @Test("Recover recovers, a click or Open Atlas opens the main window, and a dismissal does nothing")
  func eachResponseAsksForItsOwn() {
    #expect(NotificationClicks.answer(to: NotificationWords.recoverAction, offersRecover: true) == .recover)
    #expect(NotificationClicks.answer(to: NotificationWords.openAction) == .open)
    #expect(NotificationClicks.answer(to: UNNotificationDefaultActionIdentifier) == .open)
    #expect(NotificationClicks.answer(to: UNNotificationDismissActionIdentifier) == .none)
  }

  @Test("with Recover no longer offered, a Recover left in Notification Center opens Atlas instead")
  func recoverOpensWhenNotOffered() {
    #expect(NotificationClicks.answer(to: NotificationWords.recoverAction, offersRecover: false) == .open)
  }

  @Test(
    "The helper needs you offers Recover alone, which asks for the Mac to be unlocked and never brings Atlas forward"
  )
  func recoverIsTheOneAction() throws {
    let actions = Notifier.attentionCategory(offersRecover: true)
      .actions
    #expect(actions.map(\.identifier) == [NotificationWords.recoverAction], "a click on it opens Atlas already")
    let recover = try #require(actions.first)
    #expect(recover.options.contains(.authenticationRequired))
    #expect(!recover.options.contains(.foreground))
    let alone = Notifier.attentionCategory(offersRecover: false)
      .actions
    #expect(alone.map(\.identifier) == [NotificationWords.openAction], "Open Atlas instead when Recover is off")
    #expect(try #require(alone.first).options.contains(.foreground))
  }

  @Test(
    "The helper needs you says only its reason when it offers Recover, and the logged-out notice carries no actions and no sound"
  )
  func needsYouSaysItsReason() {
    let note = NotificationWords.note(.unfinished)
    #expect(note.title == "The helper needs you")
    #expect(note.acts && note.sounds)
    #expect(note.body.hasPrefix("An action may not have finished in the helper."))
    let loggedOut = NotificationWords.note(.loggedOut)
    #expect(!loggedOut.acts)
    #expect(!loggedOut.sounds, "news the person needn't act on arrives quietly")
  }

  @Test("while Atlas is in front, a notification shows as a banner with its sound unless the main window is on screen")
  func aNotificationShowsUnlessTheViewerDoes() {
    #expect(NotificationClicks.presentation(viewerShows: false) == [.banner, .sound])
    #expect(NotificationClicks.presentation(viewerShows: true).isEmpty)
  }

  @Test("an ordered but covered viewer still allows a notification banner and sound")
  func aCoveredViewerStillAllowsNotifications() {
    let covered = WindowVisibility.shows(isVisible: true, occlusionState: [])
    #expect(NotificationClicks.presentation(viewerShows: covered) == [.banner, .sound])
    let shown = WindowVisibility.shows(isVisible: true, occlusionState: .visible)
    #expect(NotificationClicks.presentation(viewerShows: shown).isEmpty)
    let hidden = WindowVisibility.shows(isVisible: false, occlusionState: .visible)
    #expect(NotificationClicks.presentation(viewerShows: hidden) == [.banner, .sound])
  }
}
