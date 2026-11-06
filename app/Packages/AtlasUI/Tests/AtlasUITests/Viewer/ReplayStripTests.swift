import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("ReplayStrip")
struct ReplayStripTests {
  /// The thumbnails a strip asked for, in order.
  final class Asked {
    /// The names asked for.
    var names: [String] = []
  }

  /// A replay of `count` frames, the first half in Safari and the rest in Slack when `grouped`.
  ///
  /// - Parameters:
  ///   - count: How many frames.
  ///   - grouped: Whether the frames span two apps.
  /// - Returns: The replay.
  static func replay(_ count: Int, grouped: Bool) -> ViewerReplayState {
    ViewerReplayState(
      title: "Post the 0.1.1 release note",
      detail: "",
      frames: (0..<count)
        .map { index in
          ViewerReplayFrame(
            id: "\(index)",
            picture: ViewerActivityShot(name: "\(index)-before.jpg"),
            words: ViewerStepWords("Clicked"),
            detail: "",
            label: "\(index + 1)",
            app: grouped && index < count / 2 ? "Safari" : "Slack"
          )
        }
    )
  }

  /// Turns the run loop so the strip builds and requests what it now shows.
  ///
  /// - Parameter host: The hosting view.
  static func settle(_ host: NSView) {
    for _ in 0..<4 {
      RunLoop.main
        .run(until: Date(timeIntervalSinceNow: 0.02))
      host.layoutSubtreeIfNeeded()
    }
  }

  /// The strip's scroll view under `view`.
  ///
  /// - Parameter view: The hosting view.
  /// - Returns: The scroll view, if any.
  static func scrollView(in view: NSView) -> NSScrollView? {
    if let scroll = view as? NSScrollView { return scroll }
    for subview in view.subviews {
      if let scroll = scrollView(in: subview) { return scroll }
    }
    return nil
  }

  @Test(
    "a long strip asks only for the frames in view, and again for a frame let go once it scrolls back",
    arguments: [false, true]
  )
  func asksAgainOnReturn(grouped: Bool) throws {
    let asked = Asked()
    let host = NSHostingView(
      rootView: ReplayStrip(replay: Self.replay(150, grouped: grouped), width: 76) { event in
        if case .thumbnailNeeded(let name) = event { asked.names.append(name) }
      }
    )
    host.frame = NSRect(x: 0, y: 0, width: 600, height: 140)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    Self.settle(host)
    #expect(asked.names.contains("0-before.jpg"))
    #expect(!asked.names.contains("149-before.jpg"), "a frame far out of view is not asked for yet")
    #expect(asked.names.count < 40)

    let scroll = try #require(Self.scrollView(in: host))
    let end = (scroll.documentView?.frame.width ?? 0) - scroll.contentView.bounds.width
    scroll.contentView.scroll(to: NSPoint(x: end, y: 0))
    scroll.reflectScrolledClipView(scroll.contentView)
    Self.settle(host)
    #expect(asked.names.contains("149-before.jpg"), "the frames scrolled to are asked for")

    let before = asked.names.count { $0 == "0-before.jpg" }
    scroll.contentView.scroll(to: .zero)
    scroll.reflectScrolledClipView(scroll.contentView)
    Self.settle(host)
    #expect(asked.names.count { $0 == "0-before.jpg" } > before, "the first frame is asked for again on return")
  }
}
