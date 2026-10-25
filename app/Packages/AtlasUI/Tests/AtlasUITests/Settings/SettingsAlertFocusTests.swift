import Foundation
import Testing

@testable import AtlasUI

@MainActor
@Suite("Settings alert focus")
struct SettingsAlertFocusTests {
  @Test("cancelling an alert restores its latest invoking control instead of an earlier target")
  func cancellingRestoresTheLatestInvoker() async {
    let focus = SettingsAlertFocus()
    var earlierFocused = false
    var invokerFocused = false
    var headingFocused = false
    focus.remember(UUID()) { earlierFocused = true }
    focus.remember(UUID()) { invokerFocused = true }
    focus.restore { headingFocused = true }
    await Self.nativeUpdate()
    #expect(invokerFocused)
    #expect(!earlierFocused && !headingFocused)
  }

  @Test("a forgotten project's removed control falls back to the surviving heading before or during dismissal")
  func aRemovedInvokerRestoresTheHeading() async {
    for removedBeforeDismissal in [true, false] {
      let focus = SettingsAlertFocus()
      let project = UUID()
      var invokerFocused = false
      var headingFocused = false
      focus.remember(project) { invokerFocused = true }
      if removedBeforeDismissal { focus.forget(project) }
      focus.restore { headingFocused = true }
      if !removedBeforeDismissal { focus.forget(project) }
      await Self.nativeUpdate()
      #expect(headingFocused && !invokerFocused)
    }
  }

  @Test("an unrelated disappearing button cannot erase the invoker and restoration releases old focus")
  func restorationOnlyOwnsItsInvoker() async {
    let focus = SettingsAlertFocus()
    var restored = 0
    var fallback = 0
    focus.remember(UUID()) { restored += 1 }
    focus.forget(UUID())
    focus.restore { fallback += 1 }
    await Self.nativeUpdate()
    focus.restore { fallback += 1 }
    await Self.nativeUpdate()
    #expect(restored == 1 && fallback == 1)
  }

  @Test("a surviving invoker disabled before or during dismissal restores the page heading")
  func aDisabledInvokerRestoresTheHeading() async {
    for disabledBeforeDismissal in [true, false] {
      let focus = SettingsAlertFocus()
      let invoker = UUID()
      var invokerFocused = false
      var headingFocused = false
      focus.remember(invoker) { invokerFocused = true }
      if disabledBeforeDismissal { focus.setEnabled(false, for: invoker) }
      focus.restore { headingFocused = true }
      if !disabledBeforeDismissal { focus.setEnabled(false, for: invoker) }
      await Self.nativeUpdate()
      #expect(headingFocused && !invokerFocused)
    }
  }

  @Test("an invoker reenabled after dismissal regains focus despite unrelated disabled controls")
  func aReenabledInvokerRegainsFocus() async {
    let focus = SettingsAlertFocus()
    let invoker = UUID()
    var invokerFocused = false
    var headingFocused = false
    focus.remember(invoker) { invokerFocused = true }
    focus.setEnabled(false, for: invoker)
    focus.restore { headingFocused = true }
    focus.setEnabled(true, for: invoker)
    focus.setEnabled(false, for: UUID())
    await Self.nativeUpdate()
    #expect(invokerFocused && !headingFocused)
  }

  /// Waits until the dismissal's native focus update has run.
  private static func nativeUpdate() async {
    await withCheckedContinuation { continuation in
      DispatchQueue.main
        .async { continuation.resume() }
    }
  }
}
