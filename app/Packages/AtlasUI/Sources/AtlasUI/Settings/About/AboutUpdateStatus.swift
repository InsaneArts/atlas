import SwiftUI

/// What About says about updates, in the one spot under its buttons (v3 G9, `.ust`), which keeps its least height
/// while empty and sets a line 14 under its top: a green check that pops in, the
/// sparkles beside a newer version, a progress bar over the install, the problem line, or plain small print. A new
/// line replaces the last with a blur, or a fade under Reduce Motion.
struct AboutUpdateStatus: View {
  /// What it says; nil for nothing.
  let status: SettingsUpdateStatus?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ZStack {
      if let status {
        line(status)
          .padding(.top, Spacing.mPlus)
          .id(status)
          .transition(Motion.transition(AnyTransition(.blurReplace), reduceMotion: reduceMotion))
      }
    }
    .frame(maxWidth: SettingsLayout.aboutStatusWidth, minHeight: SettingsLayout.aboutStatusHeight)
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: status)
  }

  /// One kind of line.
  ///
  /// - Parameter status: What it says.
  /// - Returns: The line.
  @ViewBuilder private func line(_ status: SettingsUpdateStatus) -> some View {
    switch status {
    case .done(let text):
      StatusLine(kind: .done, text: text, popsMark: true)
    case .problem(let text):
      StatusLine(kind: .problem, text: text)
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
      Text(text)
        .textStyle(.caption)
        .foregroundStyle(Color.textTertiary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
}
