import SwiftUI

/// The bar under an action shown large, an opaque dark capsule over the shade in either appearance: Before, After and
/// Slide, the one chosen lit; Show changes, in the accent while on; Previous and Next; and Close, light. With no
/// picture after the action, the ways to compare leave the bar.
struct ActionLargeBar: View {
  /// The action shown large.
  let large: ViewerActionLarge
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The compare buttons, the steps and Close.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      if large.hasAfter {
        ForEach(ViewerCompareMode.allCases, id: \.self) { mode in
          Button(Self.title(mode)) { send(.compareAction(mode)) }
            .buttonStyle(.capsule(.overPicture, size: .mini, isOn: large.compare == mode))
            .help(Self.help(mode))
            .accessibilityAddTraits(large.compare == mode ? .isSelected : [])
        }
        Button {
          send(.toggleChanges)
        } label: {
          HStack(spacing: Spacing.xs) {
            Image(systemName: SymbolName.changes.rawValue)
            Text("Show changes")
          }
        }
        .buttonStyle(.capsule(large.showsChanges ? .accent : .overPicture, size: .mini))
        .help("Dim everything that stayed the same")
        .accessibilityAddTraits(large.showsChanges ? .isSelected : [])
      }
      step(.back, label: "Previous", help: "The action before", enabled: large.canGoBack) { send(.previousAction) }
      step(.next, label: "Next", help: "The action after", enabled: large.canGoForward) { send(.nextAction) }
      Button(String("Close")) { send(.closeAction) }
        .buttonStyle(.capsule(.primary, size: .mini))
        .help("Back to the activity")
    }
    .padding(LargeShotBarLayout.inset)
    .background(Capsule().fill(Color.fillOverPicture))
    .overlay(Capsule().strokeBorder(Color.overPictureLine, lineWidth: Stroke.hairline))
    .elevation(.floating)
    .environment(\.colorScheme, .dark)
  }

  /// A round button stepping to the action before or after.
  ///
  /// - Parameters:
  ///   - symbol: Its chevron.
  ///   - label: What VoiceOver calls it.
  ///   - help: Its tooltip.
  ///   - enabled: Whether there is an action that way.
  ///   - action: What it does.
  /// - Returns: The button.
  private func step(
    _ symbol: SymbolName,
    label: String,
    help: String,
    enabled: Bool,
    action: @escaping @MainActor () -> Void
  ) -> some View {
    Button(action: action) {
      Image(systemName: symbol.rawValue)
    }
    .buttonStyle(.capsule(.overPicture, size: .mini, outline: .circle))
    .disabled(!enabled)
    .help(help)
    .accessibilityLabel(label)
  }

  /// A way to compare's word.
  ///
  /// - Parameter mode: The way.
  /// - Returns: "Before", "After" or "Slide".
  private static func title(_ mode: ViewerCompareMode) -> String {
    switch mode {
    case .before: "Before"
    case .after: "After"
    case .slide: "Slide"
    }
  }

  /// A way to compare's tooltip.
  ///
  /// - Parameter mode: The way.
  /// - Returns: The tooltip.
  private static func help(_ mode: ViewerCompareMode) -> String {
    switch mode {
    case .before: "The screen just before (B)"
    case .after: "The screen just after (A)"
    case .slide: "Drag across the picture to compare"
    }
  }
}
