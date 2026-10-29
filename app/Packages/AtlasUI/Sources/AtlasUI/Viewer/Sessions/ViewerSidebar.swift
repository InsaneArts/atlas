import SwiftUI

/// The sessions column at the window's leading edge, beside every tab, straight on the window's ground as a source
/// list stands, with no border: Sessions over All sessions, every live session, then the sessions that ended under
/// their own heading, dimmed, scrolling when the window is short. The row whose work shows is selected; a click on
/// another shows its work instead, as do ↑ and ↓ once a row has the keyboard focus, moving the focus with the choice.
/// VoiceOver reads it as one group, each row a button that says whether it is selected.
struct ViewerSidebar: View {
  /// What the column lists.
  let state: ViewerSessionsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The row with the keyboard focus, if one has it.
  @FocusState private var focused: ViewerSessionChoice?

  /// The headings and rows, scrolling when short.
  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: ViewerLayout.sidebarRowGap) {
        heading("Sessions")
        row(.all) {
          SidebarAllLabel(icons: state.icons, count: state.live.count)
        }
        ForEach(state.live) { session in
          row(.one(session.id)) {
            SidebarSessionLabel(row: session)
          }
        }
        if !state.ended.isEmpty {
          heading("Ended")
            .padding(.top, ViewerLayout.sidebarSectionGap)
          ForEach(state.ended) { session in
            row(.one(session.id)) {
              SidebarSessionLabel(row: session, isEnded: true)
            }
          }
        }
      }
      .padding(.bottom, ViewerLayout.sidebarSectionGap)
    }
    .scrollIndicators(.automatic)
    .scrollBounceBehavior(.basedOnSize)
    .frame(width: ViewerLayout.sidebarWidth)
    .frame(maxHeight: .infinity, alignment: .top)
    .onMoveCommand(perform: move)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Sessions"))
  }

  /// A heading over the column's rows, in line with their content.
  ///
  /// - Parameter text: The heading.
  /// - Returns: The heading.
  private func heading(_ text: String) -> some View {
    GroupLabel(text: text)
      .padding(ViewerLayout.sidebarHeadingInset)
  }

  /// A row that shows `choice`'s work when pressed, selected while it shows.
  ///
  /// - Parameters:
  ///   - choice: Whose work the row shows.
  ///   - content: What the row shows.
  /// - Returns: The row.
  private func row(_ choice: ViewerSessionChoice, @ViewBuilder content: () -> some View) -> some View {
    SidebarRow(isSelected: state.chosen == choice) {
      send(.chooseSession(choice))
    } content: {
      content()
    }
    .focused($focused, equals: choice)
  }

  /// ↑ or ↓ with a row focused: the row above or below shows its work and takes the focus; ← and → do nothing.
  ///
  /// - Parameter direction: The arrow pressed.
  private func move(_ direction: MoveCommandDirection) {
    let step: Int
    switch direction {
    case .up: step = -1
    case .down: step = 1
    default: return
    }
    let choice = state.choice(step, from: focused ?? state.chosen)
    focused = choice
    if choice != state.chosen { send(.chooseSession(choice)) }
  }
}

#if DEBUG
  #Preview("Sessions, one ended") {
    if let sessions = PreviewStates.viewerSessionsEnded.sessions {
      ViewerSidebar(state: sessions) { _ in }
        .frame(height: PreviewStates.viewerSize.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerSessionsEnded.sessions
    if let shown = state {
      ViewerSidebar(state: shown) { event in
        if case .chooseSession(let choice) = event { state?.chosen = choice }
      }
      .frame(height: PreviewStates.viewerSize.height)
      .sampleSurface()
    }
  }
#endif
