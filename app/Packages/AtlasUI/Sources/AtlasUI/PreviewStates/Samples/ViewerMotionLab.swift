#if DEBUG
  import SwiftUI

  /// The main window's motion, to watch and interrupt in a preview: a button for each transition
  /// (``ViewerMotionMove``), the window's own and Activity's and Apps', a burst of ten footer lines a second, a switch
  /// that turns each move back 120 ms after it starts, to show it reverses from where it is, and one that plays the
  /// last move again every 1.5 seconds.
  struct ViewerMotionLab: View {
    /// The window as drawn.
    @State private var state = PreviewStates.viewerSessions
    /// The move played last, which auto-repeat plays again.
    @State private var last: ViewerMotionMove?
    /// Whether each move is turned back mid-way.
    @State private var reverses = false
    /// Whether the last move plays again and again.
    @State private var repeats = false
    /// Counts the footer lines a burst has drawn.
    @State private var line = 0

    @Environment(\.motionReduced) private var reduceMotion

    /// The buttons over the window.
    var body: some View {
      InteractivePreview {
        DisclosureGroup(String("Segmented control motion and key frames")) {
          ScrollView {
            TabsMotionLab()
          }
          .frame(maxHeight: PreviewStates.viewerSize.height)
        }
        ForEach(Self.rows, id: \.self) { row in
          HStack(spacing: Spacing.s) {
            ForEach(row) { move in
              Button(move.rawValue) { play(move) }
            }
          }
        }
        HStack(spacing: Spacing.s) {
          Button(String("Line burst")) { Task { await burst() } }
          Toggle(String("Reverse mid-way"), isOn: $reverses)
          Toggle(String("Auto-repeat"), isOn: $repeats)
        }
      } content: {
        ViewerView(state: state) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      }
      .task(id: repeats) { await repeatLast() }
    }

    /// Plays `move`, and turns it back mid-way when asked.
    ///
    /// - Parameter move: The transition.
    private func play(_ move: ViewerMotionMove) {
      last = move
      step(move)
      guard reverses else { return }
      Task {
        try? await Task.sleep(for: Self.reversal)
        step(move)
      }
    }

    /// Plays `move` once, in the animation it runs in when the person makes it.
    ///
    /// - Parameter move: The transition.
    private func step(_ move: ViewerMotionMove) {
      withAnimation(move.animation(on: state, reduceMotion: reduceMotion)) {
        state = move.played(on: state)
      }
    }

    /// The moves, a row of buttons for every ``rowLength``.
    private static let rows = stride(from: 0, to: ViewerMotionMove.allCases.count, by: rowLength)
      .map {
        Array(
          ViewerMotionMove.allCases.dropFirst($0)
            .prefix(rowLength)
        )
      }

    /// Plays the last move again every 1.5 seconds while auto-repeat is on.
    private func repeatLast() async {
      while repeats, !Task.isCancelled {
        if let last { play(last) }
        try? await Task.sleep(for: Self.repetition)
      }
    }

    /// Ten new footer lines, one every 0.1 seconds, as an agent clicking quickly makes them.
    private func burst() async {
      for _ in 0..<Self.burstLines {
        line += 1
        state.footer.latest = ViewerLatestLine(id: "burst-\(line)", words: "Clicked button \(line)", age: "just now")
        try? await Task.sleep(for: Self.burstGap)
      }
    }

    /// 6: how many moves a row of buttons holds.
    private static let rowLength = 6
    /// 120 ms: how long after a move starts it is turned back.
    private static let reversal = Duration.milliseconds(120)
    /// 1.5 s: how often auto-repeat plays the last move again.
    private static let repetition = Duration.milliseconds(1500)
    /// 10: how many lines a burst draws.
    private static let burstLines = 10
    /// 0.1 s: how far apart a burst's lines come.
    private static let burstGap = Duration.milliseconds(100)
  }
#endif
