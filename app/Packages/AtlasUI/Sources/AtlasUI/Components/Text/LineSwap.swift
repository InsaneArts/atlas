import SwiftUI

/// Swaps one line of words for the next with no overlap, as the latest action under the helper's screen changes: the
/// old line fades out over ``Motion/Duration/swapOut``, eased in, and only once it has gone does the new one fade in
/// over ``Motion/Duration/swapIn``, eased out, so the two are never drawn together. A line holds for at least
/// ``Motion/Duration/lineHold`` once it starts to show; lines that come faster wait, only the newest of them, and the
/// rest are skipped (``LineSwapQueue``). Action labels skip that hold and keep activation disabled until their
/// replacement settles. Each fade turns round from where it is when the line it hides comes back.
/// Lines are known by their ids, so the drawn line's own details, such as its age, update in place with no
/// transition. Only opacity changes, each step within ``Motion/Duration/reduced``, so Reduce Motion keeps it as it is.
struct LineSwap<Value: Equatable, ID: Equatable, Line: View>: View {
  /// The newest line.
  private let value: Value
  /// Which line a value is, so one with new details is not a new line.
  private let id: KeyPath<Value, ID>
  /// Draws a line.
  private let line: (Value, Bool, Bool) -> Line
  /// Action labels replace obsolete words immediately; status lines keep their reading time.
  private let holds: Bool
  /// Whether the whole content fades, or its caller applies opacity to just the label.
  private let fadesContent: Bool
  /// The line drawn now.
  @State private var shown: Value
  /// The newest line that arrived, which the next fade in draws.
  @State private var latest: Value
  /// Where the swap stands.
  @State private var queue: LineSwapQueue<ID>
  /// Whether the drawn line shows, faded in or out on its own animation.
  @State private var isDrawn = true
  /// Whether the current label has finished fading in.
  @State private var isSettled = true
  /// Each fade invalidates completions and holds left from an interrupted one.
  @State private var generation = 0

  /// Creates a status line with a minimum reading time.
  ///
  /// - Parameters:
  ///   - value: The newest line.
  ///   - id: Which line a value is.
  ///   - line: Draws a line.
  init(_ value: Value, id: KeyPath<Value, ID>, @ViewBuilder line: @escaping (Value) -> Line) {
    self.init(value, id: id, holds: true, fadesContent: true) { value, _, _ in line(value) }
  }

  /// Creates a swap whose content chooses which part fades. Keep this outside a Button's label: SwiftUI may
  /// build an extra, undrawn label for its shortcut, whose animation completes immediately.
  ///
  /// - Parameters:
  ///   - value: The newest line.
  ///   - id: Which line a value is.
  ///   - holds: Whether the line keeps a minimum reading time.
  ///   - fadesContent: Whether this view fades the entire content.
  ///   - line: Draws the value, its visibility and whether it is settled and current.
  private init(
    _ value: Value,
    id: KeyPath<Value, ID>,
    holds: Bool,
    fadesContent: Bool,
    @ViewBuilder line: @escaping (Value, Bool, Bool) -> Line
  ) {
    self.value = value
    self.id = id
    self.line = line
    self.holds = holds
    self.fadesContent = fadesContent
    _shown = State(initialValue: value)
    _latest = State(initialValue: value)
    _queue = State(initialValue: LineSwapQueue(shown: value[keyPath: id]))
  }

  /// The drawn line, faded as the swap stands. Readiness also checks the incoming value, before onChange runs.
  var body: some View {
    line(shown, isDrawn, isSettled && shown[keyPath: id] == value[keyPath: id])
      // A new line is drawn whole while it is clear, never morphed out of the last one's glyphs.
      .contentTransition(.identity)
      .opacity(fadesContent && !isDrawn ? 0 : 1)
      .onChange(of: value) { _, newValue in arrive(newValue) }
      .task(id: holdTicket) { await waitOutHold() }
  }

  /// The fade a line takes: in over ``Motion/swapIn`` as it shows, out over ``Motion/swapOut`` as it goes, the same
  /// under Reduce Motion, since only opacity changes.
  ///
  /// - Parameter isShowing: Whether the line fades in.
  /// - Returns: The fade.
  nonisolated static func fade(isShowing: Bool) -> Animation {
    isShowing ? Motion.swapIn : Motion.swapOut
  }

  /// How long a line holds once it starts to show, in seconds: ``Motion/Duration/lineHold``, at the rate motion plays.
  nonisolated static var hold: TimeInterval {
    Motion.paced(Motion.Duration.lineHold)
  }

  /// The hold running now, by the fade that started it, or nil while no line holds; a new one restarts the wait.
  private var holdTicket: Int? {
    queue.phase == .holding ? generation : nil
  }

  /// Takes a new line: the drawn one with new details at once, in place, and any other into the queue.
  ///
  /// - Parameter newValue: The line.
  private func arrive(_ newValue: Value) {
    latest = newValue
    let newID = newValue[keyPath: id]
    if newID == queue.shown, queue.phase != .fadingOut {
      var transaction = Transaction()
      transaction.disablesAnimations = true
      withTransaction(transaction) { shown = newValue }
    }
    receive(.arrived(newID))
  }

  /// Waits out the drawn line's hold, then lets the queue move on; a fade that starts in the meantime cancels it.
  private func waitOutHold() async {
    guard queue.phase == .holding else { return }
    try? await Task.sleep(for: .seconds(holds ? Self.hold : .zero))
    guard !Task.isCancelled else { return }
    receive(.held)
  }

  /// Moves the queue on and starts the fade its new phase asks for.
  ///
  /// - Parameter event: What happened.
  private func receive(_ event: LineSwapQueue<ID>.Event) {
    let before = queue.phase
    queue = queue.next(event, holds: holds)
    guard queue.phase != before else { return }
    switch queue.phase {
    case .fadingOut: fadeOut()
    case .holding: fadeIn()
    case .resting: break
    }
  }

  /// Fades the drawn line out; once it has gone, the waiting line takes its place.
  private func fadeOut() {
    generation += 1
    let current = generation
    isSettled = false
    withAnimation(Self.fade(isShowing: false)) {
      isDrawn = false
    } completion: {
      // A fade turned round before it ended leaves the line where the newer fade takes it.
      if generation == current { receive(.fadedOut) }
    }
  }

  /// Draws the newest line, still clear, and fades it in, which starts its hold.
  private func fadeIn() {
    generation += 1
    let current = generation
    var transaction = Transaction()
    transaction.disablesAnimations = true
    withTransaction(transaction) { shown = latest }
    withAnimation(Self.fade(isShowing: true)) {
      isDrawn = true
    } completion: {
      if generation == current { isSettled = true }
    }
  }
}

extension LineSwap where ID == Value {
  /// Creates a swap of lines that are their own ids, such as words.
  ///
  /// - Parameters:
  ///   - value: The newest line.
  ///   - line: Draws a line.
  init(_ value: Value, @ViewBuilder line: @escaping (Value) -> Line) {
    self.init(value, id: \.self, line: line)
  }

  /// Swaps an action label without a reading hold. Its owner draws the button outside the label builder, applies
  /// the visibility to its words and gates activation on readiness, keeping presentation and action together.
  ///
  /// - Parameters:
  ///   - value: The current label.
  ///   - content: Draws the label, whether it is visible and whether its action can be activated.
  init(_ value: Value, @ViewBuilder content: @escaping (Value, Bool, Bool) -> Line) {
    self.init(value, id: \.self, holds: false, fadesContent: false, line: content)
  }
}

#if DEBUG
  #Preview("Samples") {
    LineSwapSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var interval = 0.4
    @Previewable @State var step = 0
    InteractivePreview {
      Picker(String("A new line every"), selection: $interval) {
        Text(verbatim: "0.1 s")
          .tag(0.1)
        Text(verbatim: "0.4 s")
          .tag(0.4)
        Text(verbatim: "1 s")
          .tag(1.0)
      }
      .pickerStyle(.segmented)
      .fixedSize()
    } content: {
      LineSwapSamples(steps: [step])
        .task(id: interval) {
          while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(interval))
            step += 1
          }
        }
    }
  }
#endif
