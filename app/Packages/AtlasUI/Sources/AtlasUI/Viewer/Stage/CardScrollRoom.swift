import SwiftUI

/// The rules every list in the card scrolls by (``CardScroll``), in one place. The small screen floats over a list as
/// a floating window does: the list scrolls the pane's whole height under it, nothing fades and nothing ends early,
/// and the content keeps the small screen's band as room at the end it floats over, so the last row, or the first,
/// can always be scrolled wholly clear of it. Headings scroll with their rows; none is pinned.
enum CardScrollRoom {
  /// The room the content keeps at its ends: the small screen's band at the end it floats over, none elsewhere.
  ///
  /// - Parameter clearance: Where the small screen floats.
  /// - Returns: The content's insets.
  static func contentInsets(_ clearance: MiniClearance) -> EdgeInsets {
    EdgeInsets(
      top: clearance.edges.contains(.top) ? clearance.length : .zero,
      leading: .zero,
      bottom: clearance.edges.contains(.bottom) ? clearance.length : .zero,
      trailing: .zero
    )
  }

  /// How far a selected row's room reaches past it, which a step scrolls wholly into view
  /// (``View/listSelection(_:isSelected:)``): ``ViewerLayout/listRevealMargin`` all round, for its ring, and the small
  /// screen's band at the end it floats over, so the row lands clear of it.
  ///
  /// - Parameter clearance: Where the small screen floats.
  /// - Returns: The room's reach past the row on each side.
  static func revealReach(_ clearance: MiniClearance) -> EdgeInsets {
    let band = contentInsets(clearance)
    let margin = ViewerLayout.listRevealMargin
    return EdgeInsets(
      top: band.top + margin,
      leading: margin,
      bottom: band.bottom + margin,
      trailing: margin
    )
  }

  /// The row the list keeps at its top, for its scroll position, so what the person reads never moves.
  ///
  /// - Parameters:
  ///   - row: The row the person last scrolled to the top, if any.
  ///   - isAtTop: Whether the list sits at its very top.
  /// - Returns: None at the top, so the list stays there and new rows show as they come in above; else the row
  ///   scrolled to, which holds its place as rows come in above it or the list changes around it.
  static func anchor(scrolledTo row: String?, isAtTop: Bool) -> String? {
    isAtTop ? nil : row
  }

  /// Where to scroll a selected row so it stands in view clear of the small screen, scrolling only as far as it must:
  /// nil while it already stands clear of both ends' room (``revealReach(_:)``); else the anchor that brings its
  /// foot to the room at the bottom when it stands too low, or its head to the room at the top when it stands too high.
  /// A row too tall to stand clear shows from its head. A point of rounding at either edge counts as clear.
  ///
  /// - Parameters:
  ///   - row: The row's frame in the list's visible bounds, top left first.
  ///   - viewport: The list's visible height.
  ///   - clearance: Where the small screen floats.
  /// - Returns: The anchor to scroll the row to, or nil to leave the list where it is.
  static func revealAnchor(row: CGRect, viewport: CGFloat, clearance: MiniClearance) -> UnitPoint? {
    let reach = revealReach(clearance)
    let low = row.maxY > viewport - reach.bottom + Spacing.hairline
    let high = row.minY < reach.top - Spacing.hairline
    guard low || high else { return nil }
    let room = viewport - row.height
    guard room > reach.top + reach.bottom else {
      return abs(row.minY) <= Spacing.hairline ? nil : UnitPoint(x: 0.5, y: .zero)
    }
    // A row's point at `y` of its height lines up with the list's point at `y` of its height, so its head stands
    // `y × room` below the list's top.
    let head = low ? room - reach.bottom : reach.top
    return UnitPoint(x: 0.5, y: head / room)
  }

}
