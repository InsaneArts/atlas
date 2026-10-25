import SwiftUI

/// The Updates row's detail: a check result, progress, a recovery action or a live release-news link.
/// It wraps within the space beside the row's button and grows for longer messages.
struct AboutUpdateStatus: View {
  /// What it says; nil for the invitation to check.
  let status: SettingsUpdateStatus?
  /// Receives Show in Finder.
  let send: @MainActor (SettingsEvent) -> Void

  /// The current detail, aligned with the row's title.
  var body: some View {
    if let status {
      line(status)
        .frame(maxWidth: .infinity, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
        .id(status)
        .transition(.opacity)
        .animation(Motion.textSwap, value: status)
    } else {
      Text("Check for a newer version")
        .textStyle(.caption)
        .foregroundStyle(Color.textSecondary)
    }
  }

  /// One kind of detail.
  ///
  /// - Parameter status: What it says.
  /// - Returns: The detail.
  @ViewBuilder private func line(_ status: SettingsUpdateStatus) -> some View {
    switch status {
    case .working(let text):
      StatusLine(kind: .working, text: text, size: .small, textColor: .textSecondary)
    case .done(let text):
      StatusLine(kind: .done, text: text, size: .small, popsMark: true, textColor: .textSecondary)
    case .problem(let text):
      StatusLine(kind: .problem, text: text, size: .small, textColor: .textSecondary)
    case .misplaced(let text):
      VStack(alignment: .leading, spacing: Spacing.xs) {
        StatusLine(kind: .problem, text: text, size: .small, textColor: .textSecondary)
        Button("Show in Finder") { send(.showAppInFinder) }
          .buttonStyle(.textLink(.small))
      }
    case .available(let text):
      HStack(alignment: .top, spacing: Spacing.xsPlus) {
        Image(systemName: SymbolName.sparkles.rawValue)
          .font(Typography.symbol(.control))
          .foregroundStyle(Color.textLink)
          .accessibilityHidden(true)
        Text(text)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
      }
    case .installing(let text):
      VStack(alignment: .leading, spacing: Spacing.xs) {
        Text(text)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
        ProgressView()
          .progressViewStyle(.linear)
          .tint(Color.accent)
      }
    case .note(let text):
      Text(Self.linked(text))
        .textStyle(.caption)
        .foregroundStyle(Color.textSecondary)
        .tint(Color.textLink)
    }
  }

  /// Words whose links are written as Markdown, with each link live; plain words stay as they are.
  ///
  /// - Parameter text: The words.
  /// - Returns: The words, each link live.
  nonisolated static func linked(_ text: String) -> AttributedString {
    (try? AttributedString(markdown: text)) ?? AttributedString(text)
  }
}
