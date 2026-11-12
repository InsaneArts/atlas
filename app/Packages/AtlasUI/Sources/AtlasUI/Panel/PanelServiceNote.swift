import SwiftUI

/// The note under the header that a macOS service in the helper slowly slows the Mac, on a quiet tile until the person
/// says OK: what happens, in the panel's words, over what clears it, across the tile, and OK under them at the trailing
/// edge. VoiceOver reads the words as one, then OK.
struct PanelServiceNote: View {
  /// Receives OK.
  let send: @MainActor (PanelEvent) -> Void

  /// The words over OK.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text("A macOS service in the helper slows your Mac")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
        Text("A macOS bug that Atlas tries to keep paused. Log the helper out and in to clear it.")
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      Button("OK") { send(.dismissServiceNote) }
        .buttonStyle(.capsule(.secondary, size: .small))
        .help("Don't show this again")
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
    .padding(PanelLayout.noteInset)
    .background(Color.fillTile, in: RoundedRectangle(cornerRadius: PanelLayout.noteCorner, style: .continuous))
  }
}

#if DEBUG
  #Preview("Service note") {
    PanelServiceNote { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
