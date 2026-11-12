import SwiftUI

/// The note under the header that the person's apps think they are away, on a quiet tile until the person says OK:
/// what happens, in the panel's words, over the one step that fixes it, across the tile, and OK under them at the
/// trailing edge. VoiceOver reads the words as one, then OK.
struct PanelIdleNote: View {
  /// The step that fixes it.
  let fix: PanelIdleFix
  /// Receives OK.
  let send: @MainActor (PanelEvent) -> Void

  /// The words over OK.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text("Your apps think you're away")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
        Text(detail)
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      Button("OK") { send(.dismissIdleNote) }
        .buttonStyle(.capsule(.secondary, size: .small))
        .help("Hide this until it happens again")
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
    .padding(PanelLayout.noteInset)
    .background(Color.fillTile, in: RoundedRectangle(cornerRadius: PanelLayout.noteCorner, style: .continuous))
  }

  /// Why, and the step that fixes it.
  private var detail: String {
    let why = "macOS stopped counting your typing, so apps like 1Password lock on their own."
    guard let app = fix.app else {
      return "\(why) Quit the app that keeps secure keyboard entry on to fix it."
    }
    return "\(why) \(app) keeps secure keyboard entry on: quit it to fix this."
  }
}

#if DEBUG
  #Preview("Idle note, quit an app") {
    PanelIdleNote(fix: PanelIdleFix(app: "Terminal")) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
