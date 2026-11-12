import SwiftUI

/// The account macOS names at the screen and how to fix it, on a quiet tile: the words across the tile, read as one by
/// VoiceOver, over OK and Lock Screen at the trailing edge.
struct PanelWrongAccountNote: View {
  /// The other account's name.
  let name: String
  /// Receives Lock Screen and OK.
  let send: @MainActor (PanelEvent) -> Void

  /// The words over the buttons.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.sPlus) {
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
      HStack(spacing: Spacing.s) {
        Spacer(minLength: 0)
        Button("OK") { send(.dismissWrongAccountNote) }
          .buttonStyle(.capsule(.secondary, size: .small))
        Button("Lock Screen") { send(.lockForWrongAccount) }
          .buttonStyle(.capsule(.primary, size: .small))
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
