import SwiftUI

/// The panel's facts on one card, a line between each.
struct PanelFacts: View {
  /// The helper, its display and the last action.
  let facts: [Fact]

  /// Each fact on its own row.
  var body: some View {
    CardGroup(facts) { fact in
      FactRow(name: fact.name, value: fact.value)
    }
  }
}

#if DEBUG
  #Preview("Watching") {
    PanelFacts(facts: PreviewStates.panelWatching.facts)
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
