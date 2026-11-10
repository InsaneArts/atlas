import SwiftUI

/// The content card beside the Settings sidebar, as the main window's stands beside its own: one solid card in the
/// card's colour, its corners nested in the window's, with a hairline edge; the bar at its top and the route's page
/// under it.
struct SettingsCard: View {
  /// What the window shows.
  let state: SettingsState
  /// Receives the General and Activity pages as the person changed them.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The bar over the page, on the card.
  var body: some View {
    VStack(spacing: .zero) {
      SettingsTopBar(title: state.title)
      SettingsPageStage(state: state, edit: edit, send: send)
    }
    .clipShape(shape)
    .background {
      shape
        .fill(Color.fillContent)
    }
    .overlay {
      shape
        .strokeBorder(Color.strokeContent, lineWidth: Stroke.hairline)
        .allowsHitTesting(false)
    }
  }

  /// The card's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: WindowChrome.cardRadius, style: .continuous)
  }
}
