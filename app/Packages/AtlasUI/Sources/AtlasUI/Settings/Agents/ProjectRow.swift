import SwiftUI

/// One project Atlas taught (v3 G8): a folder's tile, its name over its path in monospaced type, or that its files no
/// longer have the Atlas section, and Forget, which asks first.
struct ProjectRow: View {
  /// The project.
  let project: SettingsProjectRow
  /// Asks to forget it.
  let forget: @MainActor () -> Void

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(
      project.name,
      detail: project.detail,
      detailStyle: project.detailIsPath ? .path : .plain,
      lead: .tile(.symbol(SymbolName.folder.rawValue)),
      size: .compact
    ) {
      Button("Forget", action: forget)
        .buttonStyle(.capsule(.glass, size: .mini))
        .accessibilityLabel(project.forgetLabel)
    }
  }
}
