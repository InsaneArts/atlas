import SwiftUI

/// A moment as one piece: its header, then its steps and "Show N more", in a disclosure (``ActivityDisclosure``) that
/// grows open or shrinks shut on one spring, the steps standing still in place as its edge reveals or covers them and
/// fading each row only while it fits wholly above that edge. Folding reverses the same geometry without cutting text.
/// A new step at the top of an open moment, or one leaving it, keeps the list's own
/// arrival and departure (``ActivityMotion/step(at:isAtTop:reduceMotion:)``). A moment that folds on its own, as one
/// that ends its time open, folds the same way.
struct ActivityMomentBlock: View {
  /// The moment.
  let moment: ViewerActivityMoment
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The rows drawn under the header last, which stay drawn as the moment folds over them.
  @State private var drawn: ActivityMomentContent
  /// Whether the moment was open as last drawn, so the rows that come as it opens are revealed, not faded in.
  @State private var wasOpen: Bool
  @State private var space = UUID()
  @State private var stepHadFocus = false
  @FocusState private var headerFocused: Bool
  @FocusedValue(\.activityMomentStep) private var focusedStep
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// Creates the moment.
  ///
  /// - Parameters:
  ///   - moment: The moment.
  ///   - send: Receives what the person does.
  init(moment: ViewerActivityMoment, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.moment = moment
    self.send = send
    _drawn = State(initialValue: ActivityMomentContent(steps: moment.steps, more: moment.more))
    _wasOpen = State(initialValue: moment.isOpen)
  }

  /// The header and retained rows. Folded rows cannot receive pointer, keyboard or accessibility actions.
  var body: some View {
    let rows = ActivityMomentContent(steps: moment.steps, more: moment.more)
    let shown = rows.isEmpty ? drawn : rows
    let space = space
    return ActivityDisclosure(
      share: moment.isOpen ? 1 : 0,
      below: shown.isEmpty ? .zero : ViewerLayout.momentBelow
    ) {
      ActivityMomentRow(moment: moment, send: send)
        .focused($headerFocused)
        .padding(.top, ViewerLayout.momentAbove)
        .id("moment.\(moment.id)")
      // The steps are a list of their own, each holding the list's place, so a step read in a long moment stays put as
      // steps arrive above it.
      LazyVStack(alignment: .leading, spacing: .zero) {
        ForEach(Array(shown.steps.enumerated()), id: \.element.id) { index, step in
          ActivityStepRow(step: step, send: send)
            .layoutFrame("step \(step.id)")
            .id("step.\(step.id)")
            .visualEffect { content, proxy in
              content.opacity(
                ActivityDisclosure.opacity(
                  row: CGRect(origin: .zero, size: proxy.size),
                  bounds: proxy.bounds(of: .named(space))
                )
              )
            }
            .focusedValue(\.activityMomentStep, moment.id)
            .accessibilityHidden(!moment.isOpen)
            .transition(transition(at: index))
        }
        if let more = shown.more {
          ActivityMoreRow(more: more, send: send)
            .id("more.\(more.moment)")
            .visualEffect { content, proxy in
              content.opacity(
                ActivityDisclosure.opacity(
                  row: CGRect(origin: .zero, size: proxy.size),
                  bounds: proxy.bounds(of: .named(space))
                )
              )
            }
            .focusedValue(\.activityMomentStep, moment.id)
            .accessibilityHidden(!moment.isOpen)
            .transition(transition(at: shown.steps.count))
        }
      }
      .disabled(!moment.isOpen)
      .allowsHitTesting(moment.isOpen)
      .scrollTargetLayout()
    }
    .coordinateSpace(name: space)
    .mask(alignment: .top) {
      // The capsules reach past the column on each side, so only the moment's top and bottom clip.
      Rectangle()
        .padding(.horizontal, -ViewerLayout.capsuleReach)
    }
    .transaction(value: moment.isOpen) { transaction in
      // A moment that folds on its own, with no animation of its own, folds as one the person folds.
      transaction.animation = Motion.layout(
        transaction.animation ?? ActivityMotion.fold(reduceMotion: reduceMotion),
        reduceMotion: reduceMotion
      )
    }
    .onChange(of: rows) { _, rows in
      if !rows.isEmpty { drawn = rows }
    }
    .onChange(of: moment.isOpen) { _, isOpen in
      if !isOpen, stepHadFocus {
        headerFocused = true
        stepHadFocus = false
      }
      wasOpen = isOpen
    }
    .onChange(of: focusedStep) { _, step in
      if moment.isOpen { stepHadFocus = step == moment.id }
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
