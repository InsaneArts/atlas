import SwiftUI

/// A moment as one piece: its header, then its steps and "Show N more", in a disclosure (``ActivityDisclosure``) that
/// grows open or shrinks shut on one spring, the steps standing still in place as its edge reveals or covers them and
/// fading softly with it, the moments below moving with the edge. Folding is the exact reverse: the steps it showed stay
/// drawn as it shrinks over them. A new step at the top of an open moment, or one leaving it, keeps the list's own
/// arrival and departure (``ActivityMotion/step(at:isAtTop:reduceMotion:)``). A moment that folds on its own, as one
/// that ends its time open, folds the same way.
struct ActivityMomentBlock: View {
  /// The moment.
  let moment: ViewerActivityMoment
  /// Its steps and "Show N more", none while it is folded.
  let rows: [ViewerActivityItem]
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The rows drawn under the header last, which stay drawn as the moment folds over them.
  @State private var drawn: [ViewerActivityItem]
  /// Whether the moment was open as last drawn, so the rows that come as it opens are revealed, not faded in.
  @State private var wasOpen: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// Creates the moment.
  ///
  /// - Parameters:
  ///   - moment: The moment.
  ///   - rows: Its steps and "Show N more".
  ///   - send: Receives what the person does.
  init(moment: ViewerActivityMoment, rows: [ViewerActivityItem], send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.moment = moment
    self.rows = rows
    self.send = send
    _drawn = State(initialValue: rows)
    _wasOpen = State(initialValue: moment.isOpen)
  }

  /// The header and the rows under it, as one disclosure masked at its edge.
  var body: some View {
    let shown = rows.isEmpty ? drawn : rows
    return ActivityDisclosure(
      share: moment.isOpen ? 1 : 0,
      below: shown.isEmpty ? .zero : ViewerLayout.momentBelow
    ) {
      ActivityMomentRow(moment: moment, send: send)
        .padding(.top, ViewerLayout.momentAbove)
        .id(ViewerActivityItem.moment(moment).id)
      // The steps are a list of their own, each holding the list's place, so a step read in a long moment stays put as
      // steps arrive above it.
      LazyVStack(alignment: .leading, spacing: .zero) {
        ForEach(Array(shown.enumerated()), id: \.element.id) { index, row in
          self.row(row)
            .animation(ActivityMotion.reveal(reduceMotion: reduceMotion)) { row in
              row.opacity(moment.isOpen ? 1 : 0)
            }
            .accessibilityHidden(!moment.isOpen)
            .transition(transition(at: index))
        }
      }
      .scrollTargetLayout()
    }
    .mask(alignment: .top) {
      // The capsules reach past the column on each side, so only the moment's top and bottom clip.
      Rectangle()
        .padding(.horizontal, -ViewerLayout.capsuleReach)
    }
    .transaction(value: moment.isOpen) { transaction in
      // A moment that folds on its own, with no animation of its own, folds as one the person folds.
      guard transaction.animation == nil else { return }
      transaction.animation = ActivityMotion.fold(reduceMotion: reduceMotion)
    }
    .onChange(of: rows) { _, rows in
      if !rows.isEmpty { drawn = rows }
    }
    .onChange(of: moment.isOpen) { _, isOpen in
      wasOpen = isOpen
    }
  }

  /// One row under the header: a step, or "Show N more".
  ///
  /// - Parameter row: The row.
  /// - Returns: Its view.
  @ViewBuilder private func row(_ row: ViewerActivityItem) -> some View {
    switch row {
    case .step(let step): ActivityStepRow(step: step, send: send)
    case .more(let more): ActivityMoreRow(more: more, send: send)
    case .day, .moment: EmptyView()
    }
  }

  /// How a row under the header comes and goes: none as the moment opens or folds, which the disclosure's edge shows;
  /// while it stays open, the list's own way for a new step at the top and for a row leaving.
  ///
  /// - Parameter index: The row's place under the header.
  /// - Returns: The transition.
  private func transition(at index: Int) -> AnyTransition {
    guard moment.isOpen, wasOpen else { return .identity }
    return ActivityMotion.step(at: index, isAtTop: isAtTop, reduceMotion: reduceMotion)
  }
}
