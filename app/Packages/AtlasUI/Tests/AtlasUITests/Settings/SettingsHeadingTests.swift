import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("Settings headings")
struct SettingsHeadingTests {
  @Test("Ready to act remains a VoiceOver heading beside Check Again")
  func readyToActIsAHeading() {
    let header = HelperReadyHeader {}
    #expect(header.headingTraits.contains(.isHeader))
  }

  @Test("section titles remain VoiceOver headings with or without a description")
  func sectionTitlesAreHeadings() {
    for description in [nil, "Find out what the helper needs."] {
      let section = GroupedSection(
        [GeneralRow.picture],
        title: "When something goes wrong",
        description: description
      ) { _ in
        Text("Recover")
      }
      #expect(section.headingTraits.contains(.isHeader))
    }
  }
}
