import SwiftUI

/// A row of choices: both capsule styles share one interruptible, draggable thumb. Labels retain their size and
/// accessories hang outside their measured frames.
struct Tabs<Tab: Hashable & Sendable, Accessory: View>: View {
  /// How the tabs are drawn.
  typealias Style = TabsStyle
  /// What a tab in a track shows.
  typealias Face = TabsFace

  /// The tabs and which one is selected.
  private let state: CapsuleTabsState<Tab>
  /// How the tabs are drawn.
  private let style: Style
  /// What a tab in a track shows.
  private let face: Face
  /// Receives the tab the person picks.
  private let send: @MainActor (Tab) -> Void
  /// What a tab carries beside its word, by the tab.
  private let accessory: (Tab) -> Accessory

  /// Creates a row of tabs.
  ///
  /// - Parameters:
  ///   - state: The tabs and which one is selected.
  ///   - style: How the tabs are drawn; a track by default.
  ///   - face: What a tab in a track shows; its word by default.
  ///   - send: Receives the tab the person picks.
  ///   - accessory: What a tab carries beside its word, by the tab.
  init(
    state: CapsuleTabsState<Tab>,
    style: Style = .track,
    face: Face = .word,
    send: @escaping @MainActor (Tab) -> Void,
    @ViewBuilder accessory: @escaping (Tab) -> Accessory
  ) {
    self.state = state
    self.style = style
    self.face = face
    self.send = send
    self.accessory = accessory
  }

  /// The tabs in their track.
  var body: some View {
    TabsCapsule(state: state, style: style, face: face, send: send, accessory: accessory)
  }
}

extension Tabs where Accessory == EmptyView {
  /// Creates a row of tabs that carry nothing beside their words.
  ///
  /// - Parameters:
  ///   - state: The tabs and which one is selected.
  ///   - style: How the tabs are drawn; a track by default.
  ///   - face: What a tab in a track shows; its word by default.
  ///   - send: Receives the tab the person picks.
  init(
    state: CapsuleTabsState<Tab>,
    style: Style = .track,
    face: Face = .word,
    send: @escaping @MainActor (Tab) -> Void
  ) {
    self.init(state: state, style: style, face: face, send: send) { _ in EmptyView() }
  }
}

#if DEBUG
  #Preview("Samples") {
    TabsSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var selected = TabsSamples.Pane.screen
    @Previewable @State var symbols = false
    InteractivePreview {
      Toggle(String("Symbols"), isOn: $symbols)
    } content: {
      Tabs(
        state: CapsuleTabsState(tabs: TabsSamples.tabs, selected: selected, label: "What you see"),
        face: symbols ? .symbol : .word
      ) { tab in
        selected = tab
      }
    }
  }
#endif
