import SwiftUI
import Testing

@testable import AtlasUI

@Suite("SettingsLayout")
struct SettingsLayoutTests {
  /// How large a view draws, settled, as a render draws it.
  ///
  /// - Parameter view: The view.
  /// - Returns: Its size in points.
  /// - Throws: When nothing was drawn.
  @MainActor
  private static func size(_ view: some View) throws -> CGSize {
    let renderer = ImageRenderer(
      content: view.environment(\.animatesArrivals, false)
        .environment(\.colorScheme, .dark)
    )
    renderer.scale = 1
    let image = try #require(renderer.cgImage, "nothing was drawn")
    return CGSize(width: image.width, height: image.height)
  }

  @Test("Check Again's results land top first, each 50 milliseconds after the row above")
  func checkAgainStaggersEachResult() {
    #expect(SettingsLayout.checkStagger(index: 0) == 0, "the first result lands at once")
    for index in 1..<4 {
      let gap = SettingsLayout.checkStagger(index: index) - SettingsLayout.checkStagger(index: index - 1)
      #expect(abs(gap - 0.05) < 1e-9, "row \(index)")
    }
  }

  @Test("About keeps one height whatever its update line says, so the window never grows as a check goes")
  @MainActor
  func aboutKeepsOneHeight() throws {
    let statuses: [SettingsUpdateStatus?] = [
      nil,
      .working("Checking for updates…"),
      .done("Atlas is up to date."),
      .available("Atlas 0.2.0 is available."),
      .installing("Installing Atlas 0.2.0…"),
      .problem("Atlas couldn't check for updates."),
      .misplaced("Move Atlas to the Applications folder, then try again."),
      .note("Atlas has no public release yet. Check [atlas.insanearts.io](https://atlas.insanearts.io) for news."),
    ]
    let heights = try statuses.map { status in
      try Self.size(
        AboutPane(state: SettingsAboutState(version: "Version 0.1.0", update: .check, status: status)) { _ in }
          .frame(width: SettingsLayout.formWidth)
      )
      .height
    }
    #expect(Set(heights).count == 1, "\(heights)")
  }

  @Test("About's update button keeps one width as it checks, finds a newer version and installs")
  @MainActor
  func theUpdateButtonKeepsItsWidth() throws {
    let widths = try [SettingsUpdateButton.check, .checking, .install, .installing].map { update in
      try Self.size(AboutUpdateButton(update: update) { _ in }.fixedSize()).width
    }
    #expect(Set(widths).count == 1, "\(widths)")
  }

  @Test("About's note makes its web address a live link")
  func theNoteLinksItsAddress() {
    let note = AboutUpdateStatus.linked("Check [atlas.insanearts.io](https://atlas.insanearts.io) for news.")
    #expect(String(note.characters) == "Check atlas.insanearts.io for news.")
    #expect(note.runs.contains { $0.link == URL(string: "https://atlas.insanearts.io") })
  }
}
