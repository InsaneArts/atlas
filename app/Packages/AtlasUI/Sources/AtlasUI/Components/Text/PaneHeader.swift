import SwiftUI

/// A pane's header (v3 `.vlhead`): its title, a quiet count or note beside it, and its controls at the end, over a
/// hairline. Where they don't fit on one line, as in a narrow window with Details open, the controls move to a second
/// line under the title, and there they share what width there is, a search field giving up some of its own.
struct PaneHeader<Trailing: View>: View {
  /// The pane's title, such as "Activity".
  let title: String
  /// The note beside it, such as "38 actions today".
  let meta: String
  /// The controls at the end, such as a search field and filters.
  @ViewBuilder let trailing: Trailing

  /// The title and note with the controls beside them, or the controls under them where they don't fit.
  var body: some View {
    ViewThatFits(in: .horizontal) {
      HStack(spacing: Spacing.m) {
        heading
        Spacer(minLength: .zero)
        controls
      }
      VStack(alignment: .leading, spacing: Spacing.m) {
        HStack(spacing: Spacing.m) {
          heading
        }
        controls
      }
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .lineLimit(1)
    .padding(.top, Spacing.mPlus)
    .padding(.bottom, Spacing.m)
    .padding(.horizontal, Spacing.l)
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
      .font(Typography.font(.callout))
      .foregroundStyle(Color.textSecondary)
  }

  /// The controls, in a row.
  private var controls: some View {
    HStack(spacing: Spacing.sPlus) {
      trailing
    }
  }
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
