#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("ChooseScreen")
  struct ChooseScreenTests {
    @Test("choosing an account fills its radio and nothing else changes on the row")
    func choosingFillsTheRadioAlone() throws {
      let newAccount = PreviewStates.onboardingChooseScreen
      var testAccount = newAccount
      testAccount.isNewAccountPicked = false
      testAccount.accounts[0].isSelected = true
      let before = try PanelPixels(rows(newAccount))
      let after = try PanelPixels(rows(testAccount))
      #expect(before.width == after.width && before.height == after.height)
      // The radio's 16 points sit 14 in from the row's trailing edge; a point either side is its antialiasing.
      let radio = (before.width - 14 - 16 - 1)...(before.width - 14 + 1)
      var changed: [Int] = []
      for y in 0..<before.height {
        for x in 0..<before.width where !Self.same(before.colour(x: x, y: y), after.colour(x: x, y: y)) {
          changed.append(x)
        }
      }
      #expect(!changed.isEmpty, "the radios changed")
      #expect(changed.allSatisfy(radio.contains), "pixels changed outside the radios, at x \(Set(changed).sorted())")
    }

    @Test("one account alone is no choice, so its row has no radio, and the new account carries its Recommended tag")
    func aSingleAccountHasNoRadio() {
      let rows = ChooseScreen.rows(PreviewStates.onboardingChooseScreen)
      #expect(rows.first?.tag == "Recommended")
      #expect(rows.allSatisfy { if case .radio = $0.trailing { true } else { false } })
      guard case .screen(.choose(let only)) = PreviewStates.onboardingChooseOnly.content else {
        Issue.record("not the choice")
        return
      }
      let single = ChooseScreen.rows(only)
      #expect(single.count == 1 && single.first?.trailing == ListRow.Trailing.none)
      #expect(single.first?.titleFit == .oneLine, "an account's name keeps to one line")
    }

    /// The choice's rows in their group, as the screen draws them.
    ///
    /// - Parameter state: The choice.
    /// - Returns: The group.
    private func rows(_ state: OnboardingChooseScreenState) -> some View {
      GroupedList(ChooseScreen.rows(state)) { row in
        ListRow(row.title, detail: row.detail, lead: row.lead, trailing: row.trailing) {}
      }
      .frame(width: OnboardingLayout.column)
    }

    /// Whether two colours look the same, past rounding.
    private static func same(
      _ one: (red: Double, green: Double, blue: Double, alpha: Double),
      _ other: (red: Double, green: Double, blue: Double, alpha: Double)
    ) -> Bool {
      abs(one.red - other.red) < 0.02 && abs(one.green - other.green) < 0.02 && abs(one.blue - other.blue) < 0.02
        && abs(one.alpha - other.alpha) < 0.02
    }
  }
#endif
