import SwiftUI

/// All sessions, then each section's heading and rows, as one flat list whose lines keep their identities
/// (``SidebarSessionItem``): a session whose state changes glides from its section to its new one on ``Motion/state``
/// as the rows between open and close the gap, and a heading that comes or goes fades as its room opens or closes. Only
/// the list's order moves it, never a row's changing line. Ended's rows come in from under its heading and go back up
/// under it. The selection moves to the row chosen at once; ↑ and ↓ step through the rows in the order they
/// stand once one has the keyboard focus, moving the focus with the choice. A right click on a session's row opens its
/// menu: its Activity and screenshots, Send Key, Recover… and Clean Up This Session's Work… as they apply, and its
/// facts. Under Reduce Motion rows and headings only fade, briefly.
struct SidebarSessionList: View {
  /// The sessions.
  let state: ViewerSessionsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The row with the keyboard focus, if one has it.
  @FocusState private var focused: ViewerSessionChoice?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// All sessions over the sections' lines.
  var body: some View {
    let items = SidebarSessionItem.items(state, reduceMotion: reduceMotion)
    VStack(alignment: .leading, spacing: ViewerLayout.sidebarRowGap) {
      row(.all) { _ in
        SidebarAllLabel(icons: state.icons, count: state.liveCount)
      }
      ForEach(items) { item in
        line(item)
      }
    }
    .animation(Self.layoutMotion(reduceMotion: reduceMotion), value: items.map(\.id))
    .onMoveCommand(perform: move)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Sessions"))
  }

  /// One line: a heading, Ended's as its button, or a session's row.
  ///
  /// - Parameter item: The line.
  /// - Returns: The line, coming and going as its kind does.
  @ViewBuilder private func line(_ item: SidebarSessionItem) -> some View {
    switch item {
    case .heading(.ended):
      SidebarEndedHeading(count: state.endedCount, isOpen: state.endedShown, send: send)
        .transition(Self.arrival(rises: false, reduceMotion: reduceMotion))
    case .heading(let section):
      SidebarSectionHeading(section: section)
        .transition(Self.arrival(rises: false, reduceMotion: reduceMotion))
    case .row(let session, let section, _):
      row(.one(session.id)) { ground in
        SidebarSessionLabel(row: session, ground: ground)
      }
      .contextMenu {
        CommandMenu(sections: session.menu) { send(.menu($0)) }
      }
      .transition(Self.arrival(rises: section == .ended, reduceMotion: reduceMotion))
    }
  }

  /// A row that shows `choice`'s work when pressed, selected while it shows.
  ///
  /// - Parameters:
  ///   - choice: Whose work the row shows.
  ///   - content: What the row shows, given what it rests on.
  /// - Returns: The row.
  private func row(_ choice: ViewerSessionChoice, @ViewBuilder content: (Color) -> some View) -> some View {
    let isSelected = state.chosen == choice
    return SidebarRow(isSelected: isSelected) {
      send(.chooseSession(choice))
    } content: {
      // The badge's ring takes the pill's colour at once, as the pill comes, never easing on a white pill.
      content(isSelected ? Color.groundSelected : Color.groundRecessed)
        .transaction(value: isSelected) { $0.animation = nil }
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

  /// How the lines move as the service changes their order: on ``Motion/state``; no layout animation under
  /// Reduce Motion. Inserted and removed rows keep their own opacity transitions.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  nonisolated static func layoutMotion(reduceMotion: Bool) -> Animation? {
    Motion.animation(Motion.state, reduceMotion: reduceMotion)
  }

  /// How a line comes and goes: fading in on ``Motion/arrive`` and out on ``Motion/leave``, an Ended row coming down
  /// from under its heading and going back up under it; only fading under Reduce Motion.
  ///
  /// - Parameters:
  ///   - rises: Whether the line comes from under the heading over it, as Ended's rows do.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  nonisolated static func arrival(rises: Bool, reduceMotion: Bool) -> AnyTransition {
    let fade = AnyTransition.asymmetric(
      insertion: .opacity.animation(
        Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced)
      ),
      removal: .opacity.animation(
        Motion.animation(Motion.leave, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced)
      )
    )
    guard rises, !reduceMotion else { return fade }
    return fade.combined(with: .offset(y: -ViewerLayout.endedRise))
  }
}

#if DEBUG
  #Preview("By state") {
    SidebarSessionList(state: PreviewStates.sessionsByState) { _ in }
      .padding(.horizontal, ViewerLayout.sidebarInset)
      .frame(width: ViewerLayout.sidebarWidth)
      .background(Color.groundRecessed)
  }
#endif
