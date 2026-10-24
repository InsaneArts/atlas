import SwiftUI

/// Details' row for how old the newest picture is. It reads the age itself, in its own body, so as the app's clock
/// ticks this row alone draws again, never the column around it.
struct NewestPictureRow: View {
  /// Reads how old the newest picture is, such as "just now".
  let newest: @MainActor () -> String

  /// The age, read as the row draws.
  var body: some View {
    FactRow("Newest", value: newest())
  }
}

#if DEBUG
  #Preview("Samples") {
    NewestPictureRow { "just now" }
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
