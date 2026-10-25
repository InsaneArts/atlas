import SwiftUI

/// A row with chained calls run together.
struct Row: View {
  let names: [String]

  var body: some View {
    VStack {
      Text(verbatim: "a").font(.body).padding()
    }.frame(maxWidth: .infinity)
  }
}
