import SwiftUI

/// A row of a settings form: a title, what it means under it in small print, and its control on the right. A mark
/// or an icon may lead the title on its line; what it means starts at the row's edge, under both.
struct FormRow<Trailing: View>: View {
  /// What leads the title on its line.
  enum Accessory: Sendable {
    /// Nothing.
    case none
    /// A status's mark in its tone, such as the helper's dot.
    case status(StatusMark, StatusTone)
    /// A readiness check's mark.
    case readiness(ReadinessState)
    /// An icon, such as an agent's: its app's own, or a small tile.
    case icon(RowIcon)
  }

  /// The row's title.
  let title: String
  /// What it means, in small print; empty for none.
  var detail = ""
  /// What leads the title.
  var accessory = Accessory.none
  /// What VoiceOver reads after the title and detail, such as a status; empty for nothing. It stays on the label, so
  /// the control on the right reads only its own name.
  var spokenValue = ""
  /// The control on the right, such as a switch, a popup or a button.
  @ViewBuilder let trailing: Trailing

  var body: some View {
    HStack(spacing: Spacing.l) {
      VStack(alignment: .leading, spacing: Spacing.xxs) {
        HStack(spacing: accessoryGap) {
          lead
          Text(title)
            .font(Typography.font(.body))
            .foregroundStyle(Color.textPrimary)
        }
        if !detail.isEmpty {
          Text(detail)
            .font(Typography.font(.caption))
            .foregroundStyle(Color.textTertiary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
      .accessibilityElement(children: .combine)
      .accessibilityValue(spokenValue)
      Spacer(minLength: Spacing.m)
      HStack(spacing: Spacing.m) {
        trailing
      }
    }
    // The board's least height is its content's, with the 6 above and below outside it, so a row stands 56.
    .frame(minHeight: ControlHeight.large)
    .padding(.horizontal, Spacing.xl)
    .padding(.vertical, Spacing.s)
  }

  /// The mark or tile before the title.
  @ViewBuilder private var lead: some View {
    switch accessory {
    case .none:
      EmptyView()
    case .status(let mark, let tone):
      StatusDot(mark: mark, tone: tone)
    case .readiness(let state):
      ReadinessMark(state: state)
    case .icon(.tile(let icon)):
      SymbolTile(icon: icon)
    case .icon(.app(let icon)):
      Image(decorative: icon.image, scale: 1)
        .resizable()
        .interpolation(.high)
        .frame(width: IconSize.agent, height: IconSize.agent)
    }
  }

  /// The gap between the lead and the title: wider beside a tile than beside a mark.
  private var accessoryGap: CGFloat {
    switch accessory {
    case .icon: Spacing.ml
    case .none, .status, .readiness: Spacing.m
    }
  }
}

extension FormRow where Trailing == EmptyView {
  /// A row with nothing on its right, such as a check that passed.
  ///
  /// - Parameters:
  ///   - title: The row's title.
  ///   - detail: What it means, in small print; empty for none.
  ///   - accessory: What leads the title.
  ///   - spokenValue: What VoiceOver reads after the title and detail; empty for nothing.
  init(title: String, detail: String = "", accessory: Accessory = .none, spokenValue: String = "") {
    self.init(title: title, detail: detail, accessory: accessory, spokenValue: spokenValue) { EmptyView() }
  }
}

#if DEBUG
  #Preview("Samples") {
    FormRowSamples()
  }
#endif
