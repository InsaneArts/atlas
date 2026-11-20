import SwiftUI

/// What a small popup button draws: the choice's word and a chevron on a quiet capsule with no edge, pressed while its
/// menu is open.
struct PopupButtonLabel: View {
  /// The choice's word.
  let title: String
  /// Whether its menu is open, which fills the capsule as pressed.
  var isOn = false

  /// The word and the chevron on the capsule.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      Text(title)
        .font(Typography.font(.callout))
      Image(systemName: SymbolName.popup.rawValue)
        .font(Typography.font(.footnote, weight: .semibold))
        .accessibilityHidden(true)
    }
    .foregroundStyle(Color.textPrimary)
    .padding(.horizontal, Spacing.sPlus)
    .frame(height: ControlHeight.xs)
    .background(
      Capsule()
        .fill(isOn ? Color.fillControlOn : Color.fillControl)
    )
    .contentShape(Capsule())
  }
}
