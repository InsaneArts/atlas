import AppKit
import SwiftUI

/// An empty view in a ``CardScroll``'s content that hands the list's scroll view to its ``CardScrollPlace``.
struct CardScrollReach: NSViewRepresentable {
  /// Where the list's rows stand.
  let place: CardScrollPlace

  /// An empty view.
  ///
  /// - Parameter context: The representable's context.
  /// - Returns: The view.
  func makeNSView(context: Context) -> NSView {
    NSView()
  }

  /// Hands the scroll view the view stands in to the place.
  ///
  /// - Parameters:
  ///   - view: The view.
  ///   - context: The representable's context.
  func updateNSView(_ view: NSView, context: Context) {
    // The first update comes before the view stands in the list: look once it does.
    Task { @MainActor [place] in
      place.scroll = view.enclosingScrollView ?? place.scroll
    }
  }
}
