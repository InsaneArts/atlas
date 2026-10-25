import SwiftUI

/// The place a setup screen keeps for its status line, under its main block: the line when there is one, else the room
/// for one line, held empty. So a line that appears later, such as a problem or the work under way, moves nothing on
/// the screen, and a screen measured against its window counts the line before it shows.
struct StatusSlot<Line: View>: View {
  /// The line, which may draw nothing.
  private let line: Line

  /// Creates the slot for a line the screen draws itself.
  ///
  /// - Parameter line: The line, which may draw nothing.
  init(@ViewBuilder line: () -> Line) {
    self.line = line()
  }

  /// The line over one empty line's room, below the main block in the window's rhythm.
  var body: some View {
    ZStack {
      // One line of the status line's text, never seen, holding the slot's height.
      Text(verbatim: " ")
        .textStyle(.callout)
        .hidden()
      line
    }
    .padding(.top, OnboardingLayout.Rhythm.content)
  }
}

extension StatusSlot where Line == StatusLine? {
  /// Creates the slot for a screen's work note.
  ///
  /// - Parameter note: The window's work note: the line, or none.
  init(note: WorkNote) {
    self.line = StatusLine(note: note)
  }

  /// Creates the slot for a status line the screen chose itself.
  ///
  /// - Parameter line: The line, or nil for none.
  init(line: StatusLine?) {
    self.line = line
  }
}

#if DEBUG
  #Preview("Empty, then a line") {
    VStack(spacing: .zero) {
      StatusSlot(note: .none)
        .border(Color.separator)
      StatusSlot(note: .problem(String("Atlas couldn’t check your agents.")))
        .border(Color.separator)
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
