#if DEBUG
  import SwiftUI

  /// The soft keycaps a list draws, as single caps and as chords, so their lip and their gaps can be seen.
  struct KeycapSamples: View {
    /// The chords drawn, one a row.
    static let chords = [["⌘", "S"], ["⌃", "⌥", "⇧", "⌘", "K"], ["fn", "F11"], ["Page Up"], ["⎋"]]

    /// Each chord on its own row, then two chords as a step shows them.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(Self.chords.indices, id: \.self) { index in
          KeyChord(keys: Self.chords[index])
        }
        HStack(spacing: ViewerLayout.chordGap) {
          KeyChord(keys: ["⌘", "A"])
          KeyChord(keys: ["⌘", "C"])
        }
      }
      .sampleSurface()
    }
  }
#endif
