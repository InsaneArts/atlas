import SwiftUI

/// Places the identity above the actions when the account content is narrow.
struct HelperAccountLayout: Layout {
  /// Measures the account using the arrangement that fits the offered width.
  ///
  /// - Parameters:
  ///   - proposal: The offered account size.
  ///   - subviews: The identity and actions.
  ///   - cache: Unused; each arrangement measures its own subviews.
  /// - Returns: The account size.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let layout = layout(width: proposal.width, subviews: subviews)
    var arrangement = layout.makeCache(subviews: subviews)
    return layout.sizeThatFits(proposal: proposal, subviews: subviews, cache: &arrangement)
  }

  /// Places the identity and actions in the measured arrangement.
  ///
  /// - Parameters:
  ///   - bounds: The account content bounds.
  ///   - proposal: The offered account size.
  ///   - subviews: The identity and actions.
  ///   - cache: Unused; each arrangement places its own subviews.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    let layout = layout(width: bounds.width, subviews: subviews)
    var arrangement = layout.makeCache(subviews: subviews)
    layout.placeSubviews(in: bounds, proposal: proposal, subviews: subviews, cache: &arrangement)
  }

  /// The arrangement for the available account content width.
  ///
  /// - Parameters:
  ///   - width: The offered width, or nil for the natural size.
  ///   - subviews: The identity and actions, measured at their natural widths.
  /// - Returns: The horizontal or compact vertical arrangement.
  private func layout(width: CGFloat?, subviews: Subviews) -> AnyLayout {
    let naturalWidth = subviews.reduce(CGFloat.zero) { $0 + $1.sizeThatFits(.unspecified).width } + Spacing.m
    return if let width, width < max(naturalWidth, SettingsLayout.accountStackThreshold) {
      AnyLayout(VStackLayout(alignment: .leading, spacing: Spacing.m))
    } else {
      AnyLayout(HStackLayout(spacing: Spacing.m))
    }
  }
}
