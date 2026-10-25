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

  @Test("Every About state fits a narrow Settings window and lets longer update details wrap")
  @MainActor
  func aboutFitsANarrowColumn() throws {
    for width in [PreviewStates.settingsNarrowWidth, SettingsLayout.width] {
      let sizes = try PreviewStates.settingsAboutSamples
        .map { sample in
          try Self.size(
            AboutPane(state: sample.value.about) { _ in }
              .padding(SettingsLayout.aboutInsets)
              .frame(width: width)
          )
        }
      #expect(sizes.allSatisfy { $0.width == width }, "the column stays inside \(width) points")
      #expect(
        sizes.allSatisfy { $0.height + SettingsLayout.header < SettingsLayout.firstHeight },
        "every state fits without scrolling: \(sizes)"
      )
      if width == PreviewStates.settingsNarrowWidth {
        #expect(try #require(sizes.last).height > sizes[0].height, "long details grow instead of clipping")
      }
    }
  }

  @Test("About's update button keeps one width and the row capsule height as it checks, finds a version and installs")
  @MainActor
  func theUpdateButtonKeepsItsWidth() throws {
    let sizes = try [SettingsUpdateButton.check, .checking, .install, .installing].map { update in
      try Self.size(
        AboutUpdateButton(update: update) { _ in }
          .fixedSize()
      )
    }
    let widths = sizes.map(\.width)
    #expect(Set(widths).count == 1, "\(widths)")
    let gettingStarted = try Self.size(
      ListRowTrailing(trailing: .button("Open…"), action: {})
        .fixedSize()
    )
    #expect(sizes.allSatisfy { $0.height == gettingStarted.height }, "\(sizes), Getting Started: \(gettingStarted)")
  }

  @Test("About's note makes its web address a live link")
  func theNoteLinksItsAddress() {
    let note = AboutUpdateStatus.linked("Check [atlas.insanearts.io](https://atlas.insanearts.io) for news.")
    #expect(String(note.characters) == "Check atlas.insanearts.io for news.")
    #expect(note.runs.contains { $0.link == URL(string: "https://atlas.insanearts.io") })
  }
}
