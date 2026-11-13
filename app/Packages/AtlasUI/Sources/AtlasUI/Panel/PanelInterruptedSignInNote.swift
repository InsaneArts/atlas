import SwiftUI

/// A sign-in Atlas was holding when it stopped, on a quiet tile: the words across the tile, read as one by VoiceOver,
/// over Cancel and, when Atlas can still tell which sign-in it was, Done, at the trailing edge.
struct PanelInterruptedSignInNote: View {
  /// The sign-in, as far as it is known.
  let signIn: PanelInterruptedSignIn
  /// Receives Done and Cancel.
  let send: @MainActor (PanelEvent) -> Void

  /// The words over the buttons.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.sPlus) {
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text(title)
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
        Text(
          "The helper's screen stays hidden from agents until you say whether you finished. Agents wait until then."
        )
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
      }
      .fixedSize(horizontal: false, vertical: true)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      if let id = signIn.id {
        HStack(spacing: Spacing.s) {
          Spacer(minLength: 0)
          Button("Cancel") { send(.cancelInterruptedSignIn(id: id)) }
            .buttonStyle(.capsule(.secondary, size: .small))
          if signIn.canFinish {
            Button("Done") { send(.finishInterruptedSignIn(id: id)) }
              .buttonStyle(.capsule(.primary, size: .small))
          }
        }
      }
    }
    .padding(PanelLayout.noteInset)
    .background(Color.fillTile, in: RoundedRectangle(cornerRadius: PanelLayout.noteCorner, style: .continuous))
  }

  /// The heading, naming the site when it is known.
  private var title: String {
    guard let site = signIn.site else {
      return "A sign-in was interrupted"
    }
    return "Signing in to \(site) was interrupted"
  }
}

#if DEBUG
  #Preview("Interrupted sign-in") {
    PanelInterruptedSignInNote(signIn: PanelInterruptedSignIn(site: "github.com", canFinish: true, id: "a1")) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Interrupted sign-in, unknown") {
    PanelInterruptedSignInNote(signIn: PanelInterruptedSignIn(site: nil, canFinish: false, id: "a1")) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
