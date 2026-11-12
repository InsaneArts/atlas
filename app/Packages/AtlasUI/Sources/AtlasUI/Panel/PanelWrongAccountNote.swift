import SwiftUI

/// The account macOS names at the screen and how to fix it, with the words read as one by VoiceOver.
struct PanelWrongAccountNote: View {
  /// The other account's name.
  let name: String
  /// Receives Lock Screen and OK.
  let send: @MainActor (PanelEvent) -> Void

  /// The words on the leading edge and the buttons on the trailing edge.
  var body: some View {
    HStack(spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text("macOS thinks \(name) is at your screen")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
        Text(
          "That can make a macOS service in your login restart over and over and slowly slow your Mac. Lock your screen, choose Switch User, and log back in as yourself to fix it."
        )
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      VStack(spacing: Spacing.sPlus) {
        Button("Lock Screen") { send(.lockForWrongAccount) }
          .buttonStyle(.capsule(.primary, size: .small))
        Button("OK") { send(.dismissWrongAccountNote) }
          .buttonStyle(.capsule(.secondary, size: .small))
      }
    }
    .padding(PanelLayout.noteInset)
    .background(Color.fillTile, in: RoundedRectangle(cornerRadius: PanelLayout.noteCorner, style: .continuous))
  }
}

#if DEBUG
  #Preview("Wrong account") {
    PanelWrongAccountNote(name: "atlas") { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
