#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("DesktopScreen")
  struct DesktopScreenTests {
    @Test("Second Login and VM are choices with a grey Beta; Cloud is dimmed with its words and no radio")
    func theRowsOfTheChoice() {
      let rows = DesktopScreen.rows(PreviewStates.onboardingDesktopScreen)
      #expect(rows.map(\.title) == ["Second Login", "VM", "Cloud"])
      #expect(
        rows.map(\.detail) == [
          "A hidden second login on this Mac.", "A virtual Mac on this Mac.", "A Mac in the cloud.",
        ]
      )
      #expect(rows.map(\.tag) == ["Beta", "Beta", nil])
      #expect(rows.allSatisfy { $0.tagTone == .quiet })
      #expect(rows[0].trailing == .radio(isOn: true))
      #expect(rows[1].trailing == .radio(isOn: false))
      #expect(rows[2].trailing == .text("Not available yet"))
      #expect(rows.map(\.isNotYet) == [false, false, true])
    }

    @Test("a VM this Mac cannot run is dimmed with why at its end, and is no choice")
    func aVMThisMacCannotRunIsDimmed() {
      let rows = DesktopScreen.rows(PreviewStates.onboardingDesktopNoVMScreen)
      #expect(rows[1].trailing == .text("Needs 70 GB free"))
      #expect(rows[1].isNotYet)
      #expect(rows[1].tag == "Beta", "still Beta")
    }

    @Test("a row not offered dims its icon and words, and leaves its end whole")
    func notYetDimsTheWordsAndNotTheEnd() throws {
      let row = { (isNotYet: Bool) in
        GroupedList([ListRowItem(id: 0, title: "Cloud")]) { row in
          ListRow(
            row.title,
            detail: "A Mac in the cloud.",
            lead: .tile(.symbol("cloud")),
            trailing: .text("Not available yet"),
            isNotYet: isNotYet
          )
        }
        .frame(width: OnboardingLayout.column)
      }
      let whole = try PanelPixels(row(false))
      let dimmed = try PanelPixels(row(true))
      var changed: [Int] = []
      for y in 0..<whole.height {
        for x in 0..<whole.width where !Self.same(whole.colour(x: x, y: y), dimmed.colour(x: x, y: y)) {
          changed.append(x)
        }
      }
      // The end's words start well past the middle of the row.
      let middle = whole.width / 2
      #expect(changed.contains { $0 < middle }, "the icon and words dim")
      #expect(
        !changed.contains { $0 > middle + 40 },
        "the end stays whole, at x \(Set(changed).filter { $0 > middle }.sorted())"
      )
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
