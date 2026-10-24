import SwiftUI

/// What a control does on the left and the control on the right, such as a switch or a picker.
struct ControlRow<Label: View, Control: View>: View {
  /// How large the row is.
  enum Size: Sendable {
    /// 30 high, 8 in from each side: a row among menu rows.
    case menu
    /// 44 high, 14 in from each side: a row in a card.
    case regular
    /// As high as its label with 12 above and below, 14 in from each side: a row whose label is a title over a
    /// detail.
    case tall
  }

  /// How large the row is.
  var size = Size.regular
  /// What the control does.
  @ViewBuilder let label: Label
  /// The control.
  @ViewBuilder let control: Control

  var body: some View {
    HStack(spacing: Spacing.l) {
      label
      Spacer(minLength: Spacing.m)
      control
    }
    .padding(.horizontal, inset)
    .frame(height: height)
    .padding(.vertical, verticalInset)
  }

  /// The space on each side of the row.
  private var inset: CGFloat {
    switch size {
    case .menu: Spacing.m
    case .regular, .tall: Spacing.xl
    }
  }

  /// The space above and below a tall row's label; none for a row of fixed height.
  private var verticalInset: CGFloat {
    switch size {
    case .menu, .regular: .zero
    case .tall: Spacing.l
    }
  }

  /// The row's height, or nil for a tall row, which takes its label's.
  private var height: CGFloat? {
    switch size {
    case .menu: ControlHeight.row
    case .regular: ControlHeight.large
    case .tall: nil
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.control)
  }
#endif
