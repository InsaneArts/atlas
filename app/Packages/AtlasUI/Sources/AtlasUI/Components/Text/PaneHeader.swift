import SwiftUI

/// A pane's header: its title, a quiet count or note beside it, and its controls at the end, over a hairline. Where
/// they don't fit on one line, as in a narrow window with Details open, the controls move to a second line under the
/// title, and there they share what width there is, a search field giving up some of its own.
struct PaneHeader<Trailing: View>: View {
  /// The pane's title, such as "Activity".
  let title: String
  /// The note beside it, such as "38 actions today".
  let meta: String
  /// The controls at the end, such as a search field and filters.
  @ViewBuilder let trailing: Trailing

  var body: some View {
    ViewThatFits(in: .horizontal) {
      HStack(spacing: Spacing.l) {
        heading
        Spacer(minLength: .zero)
        controls
      }
      VStack(alignment: .leading, spacing: Spacing.l) {
        HStack(spacing: Spacing.l) {
          heading
        }
        controls
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .lineLimit(1)
    .padding(.vertical, Spacing.xl)
    .padding(.horizontal, Layout.inset)
    .overlay(alignment: .bottom) {
      SeparatorLine()
    }
  }
}

extension PaneHeader {
  /// The title and the note beside it; the note gives way first.
  @ViewBuilder private var heading: some View {
    Text(title)
      .font(Typography.font(.heading, weight: .semibold))
      .layoutPriority(1)
      .accessibilityAddTraits(.isHeader)
    Text(meta)
      .font(Typography.font(.body))
      .foregroundStyle(Color.textTertiary)
  }

  /// The controls, in a row.
  private var controls: some View {
    HStack(spacing: Spacing.ml) {
      trailing
    }
  }
}

/// At file scope because `PaneHeader` is generic.
private enum Layout {
  /// 18: the pane's text inset, the board's (mac.py `head`), which its rows share, between the grid's 16 and 20.
  static let inset: CGFloat = 18
}

extension PaneHeader where Trailing == EmptyView {
  /// A header with nothing at its end.
  ///
  /// - Parameters:
  ///   - title: The pane's title.
  ///   - meta: The note beside it.
  init(title: String, meta: String) {
    self.init(title: title, meta: meta) { EmptyView() }
  }
}

#if DEBUG
  #Preview("Samples") {
    PaneHeaderSamples()
  }
#endif
