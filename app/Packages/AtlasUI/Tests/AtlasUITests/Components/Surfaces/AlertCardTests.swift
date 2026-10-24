import SwiftUI
import Testing

@testable import AtlasUI

@Suite("AlertCard")
struct AlertCardTests {
  @Test("two labels of ten characters or fewer sit side by side with the default on the right")
  func shortLabelsSitSideBySide() {
    let buttons = [
      AlertCardButton(id: "recover", title: "Recover", role: .action),
      AlertCardButton(id: "later", title: "Not Now", role: .cancel),
    ]
    let arrangement = AlertCard<String>.arrangement(labels: buttons.map(\.title))
    #expect(arrangement == .sideBySide)
    #expect(AlertCard.ordered(buttons, in: arrangement).map(\.id) == ["later", "recover"])
    #expect(AlertCard<String>.arrangement(labels: ["Cancel", "1234567890"]) == .sideBySide, "ten exactly still fits")
  }

  @Test("a label longer than ten characters stacks the buttons with the default on top")
  func longLabelsStack() {
    let buttons = [
      AlertCardButton(id: "cancel", title: "Cancel", role: .safeCancel),
      AlertCardButton(id: "logOut", title: "Log Out Anyway", role: .destructive),
    ]
    let arrangement = AlertCard<String>.arrangement(labels: buttons.map(\.title))
    #expect(arrangement == .stacked)
    #expect(AlertCard.ordered(buttons, in: arrangement).map(\.id) == ["cancel", "logOut"])
  }

  @Test("Return presses the default and Escape the way out, whichever way the buttons sit")
  func keysFollowTheRoles() {
    #expect(AlertCardButton<String>.Role.action.shortcut == .defaultAction)
    #expect(AlertCardButton<String>.Role.cancel.shortcut == .cancelAction)
    #expect(AlertCardButton<String>.Role.safeCancel.shortcut == .defaultAction)
    #expect(AlertCardButton<String>.Role.destructive.shortcut == nil)
    #expect(AlertCardButton<String>.Role.destructive.look == .destructive, "the destructive word is red")
    #expect(AlertCardButton<String>.Role.safeCancel.look == .accent, "Cancel is the blue default")
  }

  @Test("a working button's spinner takes its own word's colour: white on the accent, red on a destructive one")
  func theSpinnerMatchesItsWord() {
    #expect(AlertCardButton<String>.Role.action.spinnerArc == .onAccent)
    #expect(AlertCardButton<String>.Role.destructive.spinnerArc == .textDestructive)
  }

  @Test("a refusal shakes the card once, and never under Reduce Motion")
  func aRefusalShakesUnlessReduceMotion() {
    #expect(AlertCard<String>.shakes(on: "The helper is in use.", reduceMotion: false))
    #expect(!AlertCard<String>.shakes(on: "The helper is in use.", reduceMotion: true))
    #expect(!AlertCard<String>.shakes(on: nil, reduceMotion: false), "the refusal clearing as it runs again")
  }
}
