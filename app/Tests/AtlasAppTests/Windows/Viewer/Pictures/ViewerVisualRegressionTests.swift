import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Viewer visual regressions", .serialized, AppKitState())
@MainActor
struct ViewerVisualRegressionTests {
  @Test("all Activity filter labels and the entire capsule fit a 320 point viewer")
  func narrowFiltersRemainWhole() throws {
    var frames: [String: CGRect] = [:]
    let report = LayoutFrameReport { frames[$0] = $1 }
    let activity = try #require(
      { if case .activity(let value) = PreviewStates.viewerActivity.pane { value } else { nil } }()
    )
    let width = 320 - ViewerLayout.cardInset * 2
    let hosting = NSHostingView(
      rootView: ActivityListContent(state: activity, send: { _ in })
        .environment(\.layoutFrameReport, report)
        .frame(width: width, height: 900)
    )
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: width, height: 900),
      styleMask: .borderless,
      backing: .buffered,
      defer: false
    )
    window.contentView = hosting
    defer { window.contentView = nil }
    hosting.layoutSubtreeIfNeeded()
    hosting._renderForTest(interval: .zero)
    let filter = try #require(frames["activity filters"])
    #expect(filter.width > 0)
    #expect(filter.minX >= ViewerLayout.swapInset)
    #expect(filter.maxX <= width - ViewerLayout.swapInset)
  }

  @Test("a front-app change draws the singular blue status on exactly one card in every animation frame")
  func frontAppStatusChangesImmediately() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let first = PreviewStates.viewerAppsSafariFront
    let host = ViewerHost(first, animatesArrivals: false)
    host.render()
    Motion.Pose.hold(at: .zero)
    host.show(PreviewStates.viewerApps)
    for time in [0.0, 0.04, 0.08, 0.12, 0.2, 0.4, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      let tags = host.frames.filter { $0.key.hasPrefix("app tags ") && $0.value.width > 0 }
      try #require(!tags.isEmpty)
      let blueCards = try tags.values.filter { try Self.ink(host.pixels(in: $0)).blue }
      #expect(blueCards.count == 1, "two cards must never claim the front app at time \(time)")
    }
  }

  @Test(
    "Show more removes the link before drawing the replacement step in its reserved place",
    arguments: [false, true],
    [NSAppearance.Name.aqua, .darkAqua]
  )
  func showMoreNeverOverprintsItsLink(reduced: Bool, appearance: NSAppearance.Name) throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let first = PreviewStates.viewerActivityOpened
    let host = ViewerHost(
      first,
      animatesArrivals: false,
      reduceMotion: reduced,
      appearance: NSAppearance(named: appearance)
    )
    host.render()
    let link = try #require(host.frames.first { $0.key.hasPrefix("more ") }?.value)
    let region = link.insetBy(dx: -2, dy: -2)
    let lightText = appearance == .darkAqua
    let before = try Self.ink(host.pixels(in: region), lightText: lightText)
    try #require(before.blue && !before.text, "the settled link alone must not count as replacement words")
    Motion.Pose.hold(at: .zero)
    withAnimation(ActivityMotion.fold(reduceMotion: reduced)) {
      host.show(ViewerMotionMove.more.played(on: first))
    }
    for time in [0.0, 0.02, 0.05, 0.08, 0.1, 0.12, 0.2, 0.4, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      let ink = try Self.ink(host.pixels(in: region), lightText: lightText)
      #expect(!(ink.blue && ink.text), "the link and replacement text must not overprint at time \(time)")
    }
    let after = try Self.ink(host.pixels(in: region), lightText: lightText)
    #expect(after.text && !after.blue, "the settled replacement must draw in the link's reserved place")
  }

  /// Counts blue link ink and neutral text ink, excluding the ground in the appearance being drawn.
  private static func ink(_ pixels: Data, lightText: Bool = false) throws -> (blue: Bool, text: Bool) {
    let bitmap = try #require(NSBitmapImageRep(data: pixels))
    var blue = 0
    var text = 0
    for y in 0..<bitmap.pixelsHigh {
      for x in 0..<bitmap.pixelsWide {
        guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { continue }
        if color.blueComponent > color.redComponent + 0.15, color.blueComponent > color.greenComponent + 0.03 {
          blue += 1
        }
        let darkest = min(color.redComponent, color.greenComponent, color.blueComponent)
        let brightest = max(color.redComponent, color.greenComponent, color.blueComponent)
        // A dark ground (and a blue glyph's dim edges on it) is not the arriving light text.
        if lightText ? darkest > 0.5 : brightest < 0.5, brightest - darkest < 0.1 {
          text += 1
        }
      }
    }
    return (blue > 5, text > 5)
  }
}
