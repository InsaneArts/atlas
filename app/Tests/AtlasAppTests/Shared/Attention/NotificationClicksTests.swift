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

  @Test("Recover asks for the Mac to be unlocked first, and never brings Atlas forward; Open Atlas does")
  func recoverNeedsUnlock() throws {
    let actions = Notifier.attentionCategory(offersRecover: true)
      .actions
    let recover = try #require(actions.first { $0.identifier == NotificationWords.recoverAction })
    #expect(recover.options.contains(.authenticationRequired))
    #expect(!recover.options.contains(.foreground))
    let open = try #require(actions.first { $0.identifier == NotificationWords.openAction })
    #expect(open.options.contains(.foreground))
    let alone = Notifier.attentionCategory(offersRecover: false)
      .actions
    #expect(alone.map(\.identifier) == [NotificationWords.openAction], "Open Atlas alone when Recover is off")
  }

  @Test("Atlas needs you says only its reason when it offers Recover, and the logged-out notice carries no actions")
  func needsYouSaysItsReason() {
    let note = NotificationWords.note(.unfinished)
    #expect(note.title == "Atlas needs you")
    #expect(note.acts)
    #expect(note.body.hasPrefix("An action may not have finished in the helper."))
    #expect(!NotificationWords.note(.loggedOut).acts)
  }
}
