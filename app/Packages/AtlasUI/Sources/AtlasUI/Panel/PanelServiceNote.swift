import SwiftUI

/// The note under the header that a macOS service in the helper slowly slows the Mac, on a quiet tile until the person
/// says OK: what happens, in the panel's words, over what clears it, and OK at the trailing edge. VoiceOver reads the
/// words as one, then OK.
struct PanelServiceNote: View {
  /// Receives OK.
  let send: @MainActor (PanelEvent) -> Void

  /// The words on the leading edge, OK on the trailing one.
  var body: some View {
    HStack(spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text("A macOS service in the helper slowly slows your Mac")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
        Text("It's a macOS bug Atlas can't turn off. Log the helper out and back in every few days to clear it.")
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      Button("OK") { send(.dismissServiceNote) }
        .buttonStyle(.capsule(.secondary, size: .small))
        .help("Don't show this again")
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
