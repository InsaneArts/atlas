import SwiftUI

/// What About says about updates, in the one spot under its buttons, which always keeps
/// ``SettingsLayout/aboutStatusHeight``, so the window never grows or shrinks as the check goes: a spinner while a
/// check runs, a green check that pops in, the sparkles beside a newer version, one progress bar over the install, the
/// problem line, with Show in Finder when Atlas must move, or plain words whose web address is a link. A new line
/// swaps in place of the last by opacity.
struct AboutUpdateStatus: View {
  /// What it says; nil for nothing.
  let status: SettingsUpdateStatus?
  /// Receives Show in Finder.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    ZStack(alignment: .top) {
      if let status {
        line(status)
          .padding(.top, Spacing.mPlus)
          .id(status)
          .transition(.opacity)
      }
    }
    .frame(maxWidth: SettingsLayout.aboutStatusWidth, alignment: .top)
    .frame(height: SettingsLayout.aboutStatusHeight, alignment: .top)
    .animation(Motion.textSwap, value: status)
  }

  /// One kind of line.
  ///
  /// - Parameter status: What it says.
  /// - Returns: The line.
  @ViewBuilder private func line(_ status: SettingsUpdateStatus) -> some View {
    switch status {
    case .working(let text):
      StatusLine(kind: .working, text: text)
    case .done(let text):
      StatusLine(kind: .done, text: text, popsMark: true)
    case .problem(let text):
      StatusLine(kind: .problem, text: text)
    case .misplaced(let text):
      VStack(spacing: Spacing.xs) {
        StatusLine(kind: .problem, text: text)
        Button("Show in Finder") { send(.showAppInFinder) }
          .buttonStyle(.textLink(.regular))
      }
    case .available(let text):
      HStack(spacing: Spacing.xsPlus) {
        Image(systemName: SymbolName.sparkles.rawValue)
          .font(Typography.symbol(.control))
          .foregroundStyle(Color.textLink)
          .accessibilityHidden(true)
        Text(text)
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
    case .installing(let text):
      VStack(spacing: Spacing.s) {
        ProgressView()
          .progressViewStyle(.linear)
          .tint(Color.accent)
          .frame(width: SettingsLayout.progressWidth)
        Text(text)
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
    case .note(let text):
      Text(Self.linked(text))
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
        .tint(Color.textLink)
        .fixedSize(horizontal: false, vertical: true)
    }
  }

  /// Words whose links are written as Markdown, such as "[atlas.insanearts.io](https://atlas.insanearts.io)", with
  /// each link live; words that are not Markdown stay as they are.
  ///
  /// - Parameter text: The words.
  /// - Returns: The words, each link live.
  nonisolated static func linked(_ text: String) -> AttributedString {
    (try? AttributedString(markdown: text)) ?? AttributedString(text)
  }
}
