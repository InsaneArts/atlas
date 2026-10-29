import SwiftUI

/// Swaps one line of words for the next with no overlap, as the latest action under the helper's screen changes: the
/// old line fades out over ``Motion/Duration/swapOut``, eased in, and only once it has gone does the new one fade in
/// over ``Motion/Duration/swapIn``, eased out, so the two are never drawn together. A line holds for at least
/// ``Motion/Duration/lineHold`` once it starts to show; lines that come faster wait, only the newest of them, and the
/// rest are skipped (``LineSwapQueue``). Each fade turns round from where it is when the line it hides comes back.
/// Lines are known by their ids, so the drawn line's own details, such as its age, update in place with no
/// transition. Only opacity changes, each step within ``Motion/Duration/reduced``, so Reduce Motion keeps it as it is.
struct LineSwap<Value: Equatable, ID: Equatable, Line: View>: View {
  /// The newest line.
  private let value: Value
  /// Which line a value is, so one with new details is not a new line.
  private let id: KeyPath<Value, ID>
  /// Draws a line.
  private let line: (Value) -> Line
  /// The line drawn now.
  @State private var shown: Value
  /// The newest line that arrived, which the next fade in draws.
  @State private var latest: Value
  /// Where the swap stands.
  @State private var queue: LineSwapQueue<ID>
  /// Whether the drawn line shows, faded in or out on its own animation.
  @State private var isDrawn = true
  /// Each fade invalidates completions and holds left from an interrupted one.
  @State private var generation = 0

  /// Creates the swap.
  ///
  /// - Parameters:
  ///   - value: The newest line.
  ///   - id: Which line a value is.
  ///   - line: Draws a line.
  init(_ value: Value, id: KeyPath<Value, ID>, @ViewBuilder line: @escaping (Value) -> Line) {
    self.value = value
    self.id = id
    self.line = line
    _shown = State(initialValue: value)
    _latest = State(initialValue: value)
    _queue = State(initialValue: LineSwapQueue(shown: value[keyPath: id]))
  }

  /// The drawn line, faded as the swap stands.
  var body: some View {
    line(shown)
      // A new line is drawn whole while it is clear, never morphed out of the last one's glyphs.
      .contentTransition(.identity)
      .opacity(isDrawn ? 1 : 0)
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
    try? await Task.sleep(for: .seconds(Self.hold))
    guard !Task.isCancelled else { return }
    receive(.held)
  }

  /// Moves the queue on and starts the fade its new phase asks for.
  ///
  /// - Parameter event: What happened.
  private func receive(_ event: LineSwapQueue<ID>.Event) {
    let before = queue.phase
    queue = queue.next(event)
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
    var transaction = Transaction()
    transaction.disablesAnimations = true
    withTransaction(transaction) { shown = latest }
    withAnimation(Self.fade(isShowing: true)) { isDrawn = true }
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
