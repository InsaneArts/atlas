import SwiftUI

/// A row led by a symbol tile: its title and detail, then whatever the row ends with, such as a switch or a status.
struct IconRow<Trailing: View>: View {
  /// What the tile shows.
  enum Icon: Sendable {
    /// An SF Symbol.
    case symbol(String)
    /// A few monospaced characters, such as a terminal's `>_`.
    case text(String)
  }

  /// What the tile shows.
  let icon: Icon
  /// The row's title.
  let title: String
  /// What it means.
  let detail: String
  /// Whether the title and detail wrap onto as many lines as they need, rather than being cut.
  var wraps = false
  /// What the row ends with.
  @ViewBuilder let trailing: Trailing

  var body: some View {
    HStack(spacing: Spacing.m) {
      tile
        .frame(width: IconSize.l, height: IconSize.l)
        .background(
          RoundedRectangle(cornerRadius: Radius.small)
            .fill(Color.fillControl)
        )
        .accessibilityHidden(true)
      TitleDetail(title: title, detail: detail, titleWraps: wraps, wraps: wraps)
      Spacer(minLength: Spacing.s)
      trailing
    }
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.m)
  }

  @ViewBuilder private var tile: some View {
    switch icon {
    case .symbol(let symbol):
      Image(systemName: symbol)
        .font(Typography.font(.headline))
    case .text(let text):
      Text(text)
        .font(Typography.font(.callout, design: .monospaced))
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.icon)
  }
#endif
